use bytemuck::{Pod, Zeroable};
use web_sys::HtmlCanvasElement;
use wgpu::util::DeviceExt;

use crate::sprite;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ShipUniform {
    pos: [f32; 2],
    angle: f32,
    /// Engine flame intensity, 0 (off) to 1 (full).
    flame: f32,
    /// Canvas size in CSS px.
    view_size: [f32; 2],
    /// Canvas backing-store size in physical px.
    fb_size: [f32; 2],
    /// Seconds, for animation.
    time: f32,
    _pad: [f32; 3],
}

/// How fast the flame flares up when thrust starts and dies down when it
/// stops, per second.
const FLAME_IGNITE_RATE: f32 = 14.0;
const FLAME_FADE_RATE: f32 = 4.0;

pub struct Gfx {
    canvas: HtmlCanvasElement,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// Animation clock, in seconds.
    time: f32,
    /// Current flame intensity, eased toward the thrust input.
    flame: f32,
}

impl Gfx {
    pub async fn new(canvas: HtmlCanvasElement) -> Self {
        // WebGPU support has to be decided at instance creation; this drops the
        // WebGPU backend when the browser lacks it, leaving WebGL2.
        let instance = wgpu::util::new_instance_with_webgpu_detection(
            wgpu::InstanceDescriptor::new_without_display_handle(),
        )
        .await;
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
            .expect("failed to create surface from canvas");
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .expect("no compatible graphics adapter");
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("device"),
                // Lowest common denominator so the same request succeeds on WebGL2.
                required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                    .using_resolution(adapter.limits()),
                ..Default::default()
            })
            .await
            .expect("failed to create device");

        let (width, height) = backing_size(&canvas, &device);
        canvas.set_width(width);
        canvas.set_height(height);
        let mut config = surface
            .get_default_config(&adapter, width, height)
            .expect("surface is not supported by the adapter");
        // Sprite colors are sRGB values meant to be written out unchanged.
        if let Some(&format) = surface
            .get_capabilities(&adapter)
            .formats
            .iter()
            .find(|format| !format.is_srgb())
        {
            config.format = format;
        }
        surface.configure(&device, &config);

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ship uniform"),
            size: size_of::<ShipUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let image = sprite::decode(&sprite::SHIP);
        let sprite_texture = device.create_texture_with_data(
            &queue,
            &wgpu::TextureDescriptor {
                label: Some("ship sprite"),
                size: wgpu::Extent3d {
                    width: image.width,
                    height: image.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &image.rgba,
        );
        let sprite_view = sprite_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("pixel art sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ship bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ship bind group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&sprite_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ship shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("ship.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ship pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ship pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(config.format.into())],
            }),
            multiview_mask: None,
            cache: None,
        });

        Self {
            canvas,
            surface,
            device,
            queue,
            config,
            pipeline,
            uniform_buffer,
            bind_group,
            time: 0.0,
            flame: 0.0,
        }
    }

    /// Match the canvas backing store to its CSS size and reconfigure the surface.
    pub fn resize(&mut self) {
        let (width, height) = backing_size(&self.canvas, &self.device);
        self.canvas.set_width(width);
        self.canvas.set_height(height);
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    /// Canvas size in CSS px, which is also the world size.
    pub fn view_size(&self) -> [f32; 2] {
        [
            self.canvas.client_width().max(1) as f32,
            self.canvas.client_height().max(1) as f32,
        ]
    }

    /// Draw `ship`. `thrusting` and `dt` (seconds since the last frame) only
    /// drive the engine flame animation.
    pub fn render(&mut self, ship: &sim::Ship, thrusting: bool, dt: f32) {
        // Wrap the clock so it never grows large enough to lose f32 precision.
        self.time = (self.time + dt) % 3600.0;
        let (target, rate) = if thrusting {
            (1.0, FLAME_IGNITE_RATE)
        } else {
            (0.0, FLAME_FADE_RATE)
        };
        self.flame += (target - self.flame) * (1.0 - (-rate * dt).exp());

        let uniform = ShipUniform {
            pos: ship.pos,
            angle: ship.angle,
            flame: self.flame,
            view_size: self.view_size(),
            fb_size: [self.config.width as f32, self.config.height as f32],
            time: self.time,
            _pad: [0.0; 3],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniform));

        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            // Timeout, occlusion or a validation error: skip this frame.
            _ => return,
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ship pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.draw(0..6, 0..1);
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
    }
}

/// Canvas CSS size × devicePixelRatio, clamped to what the device can present.
fn backing_size(canvas: &HtmlCanvasElement, device: &wgpu::Device) -> (u32, u32) {
    let dpr = web_sys::window().map_or(1.0, |w| w.device_pixel_ratio());
    let max = device.limits().max_texture_dimension_2d;
    let scale = |css: i32| ((css as f64 * dpr).round() as u32).clamp(1, max);
    (scale(canvas.client_width()), scale(canvas.client_height()))
}
