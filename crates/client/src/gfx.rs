use bytemuck::{Pod, Zeroable};
use web_sys::HtmlCanvasElement;
use wgpu::util::DeviceExt;

use crate::sprite;

/// Shared by every pipeline (group 0). Matches `View` in common.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ViewUniform {
    /// Canvas size in CSS px, which is also the world size.
    size: [f32; 2],
    /// Canvas backing-store size in physical px.
    fb_size: [f32; 2],
    /// Seconds, for animation.
    time: f32,
    _pad: [f32; 3],
}

/// Matches `Ship` in ship.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ShipUniform {
    pos: [f32; 2],
    angle: f32,
    /// Engine flame intensity, 0 (off) to 1 (full).
    flame: f32,
    /// Smoothed turn rate, rad/s, for bending the flame.
    turn: f32,
    _pad: [f32; 3],
}

/// One asteroid, as a per-instance vertex. Matches `Rock` in asteroid.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct RockInstance {
    pos: [f32; 2],
    radius: f32,
    angle: f32,
    look: f32,
    /// The rock's outline from the sim, so what's drawn is what collides.
    shape: [f32; sim::SHAPE_POINTS],
}

/// Rocks beyond this many aren't drawn.
const MAX_ROCKS: usize = 64;
/// Each object is drawn as a square in each of its 3×3 wrapped copies. Must
/// match `VERTS_PER_OBJECT` in common.wgsl.
const VERTS_PER_OBJECT: u32 = 54;

/// How fast the flame flares up when thrust starts and dies down when it
/// stops, per second.
const FLAME_IGNITE_RATE: f32 = 14.0;
const FLAME_FADE_RATE: f32 = 4.0;
/// How fast the flame's bend follows the ship's turn rate, per second: about
/// how quickly old exhaust flushes out of the plume.
const FLAME_BEND_RATE: f32 = 8.0;

pub struct Gfx {
    canvas: HtmlCanvasElement,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    view_buffer: wgpu::Buffer,
    view_bind_group: wgpu::BindGroup,
    stars_pipeline: wgpu::RenderPipeline,
    ship_pipeline: wgpu::RenderPipeline,
    ship_buffer: wgpu::Buffer,
    ship_bind_group: wgpu::BindGroup,
    rock_pipeline: wgpu::RenderPipeline,
    rock_buffer: wgpu::Buffer,
    /// Animation clock, in seconds.
    time: f32,
    /// Current flame intensity, eased toward the thrust input.
    flame: f32,
    /// Ship angle at the last frame, to measure how fast it's turning.
    last_angle: Option<f32>,
    /// Turn rate in rad/s, eased so the flame bends and straightens smoothly.
    turn: f32,
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

        let view_buffer = uniform_buffer::<ViewUniform>(&device, "view uniform");
        let view_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("view bind group layout"),
            entries: &[uniform_entry(0)],
        });
        let view_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("view bind group"),
            layout: &view_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: view_buffer.as_entire_binding(),
            }],
        });

        let stars_pipeline = pipeline(
            &device,
            "stars",
            concat!(include_str!("common.wgsl"), include_str!("stars.wgsl")),
            &[&view_layout],
            &[],
            config.format,
        );

        let ship_buffer = uniform_buffer::<ShipUniform>(&device, "ship uniform");
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
        let ship_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ship bind group layout"),
            entries: &[
                uniform_entry(0),
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
        let ship_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ship bind group"),
            layout: &ship_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: ship_buffer.as_entire_binding(),
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
        let ship_pipeline = pipeline(
            &device,
            "ship",
            concat!(include_str!("common.wgsl"), include_str!("ship.wgsl")),
            &[&view_layout, &ship_layout],
            &[],
            config.format,
        );

        let rock_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rock instances"),
            size: (MAX_ROCKS * size_of::<RockInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let rock_pipeline = pipeline(
            &device,
            "asteroid",
            concat!(include_str!("common.wgsl"), include_str!("asteroid.wgsl")),
            &[&view_layout],
            &[Some(wgpu::VertexBufferLayout {
                array_stride: size_of::<RockInstance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![
                    0 => Float32x2,
                    1 => Float32,
                    2 => Float32,
                    3 => Float32,
                    // The 16-point outline, as four vec4s.
                    4 => Float32x4,
                    5 => Float32x4,
                    6 => Float32x4,
                    7 => Float32x4,
                ],
            })],
            config.format,
        );

        Self {
            canvas,
            surface,
            device,
            queue,
            config,
            view_buffer,
            view_bind_group,
            stars_pipeline,
            ship_pipeline,
            ship_buffer,
            ship_bind_group,
            rock_pipeline,
            rock_buffer,
            time: 0.0,
            flame: 0.0,
            last_angle: None,
            turn: 0.0,
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

    /// Draw `world`. `thrusting` and `dt` (seconds since the last frame) only
    /// drive animation.
    pub fn render(&mut self, world: &sim::World, thrusting: bool, dt: f32) {
        // Wrap the clock so it never grows large enough to lose f32 precision.
        self.time = (self.time + dt) % 3600.0;
        let (target, rate) = if thrusting {
            (1.0, FLAME_IGNITE_RATE)
        } else {
            (0.0, FLAME_FADE_RATE)
        };
        self.flame += (target - self.flame) * (1.0 - (-rate * dt).exp());
        let angle = world.ship.angle;
        if let Some(last) = self.last_angle.filter(|_| dt > 0.0) {
            let turn_rate = (angle - last) / dt;
            self.turn += (turn_rate - self.turn) * (1.0 - (-FLAME_BEND_RATE * dt).exp());
        }
        self.last_angle = Some(angle);

        let view = ViewUniform {
            size: self.view_size(),
            fb_size: [self.config.width as f32, self.config.height as f32],
            time: self.time,
            _pad: [0.0; 3],
        };
        self.queue
            .write_buffer(&self.view_buffer, 0, bytemuck::bytes_of(&view));
        let ship = ShipUniform {
            pos: world.ship.pos,
            angle: world.ship.angle,
            flame: self.flame,
            turn: self.turn,
            _pad: [0.0; 3],
        };
        self.queue
            .write_buffer(&self.ship_buffer, 0, bytemuck::bytes_of(&ship));
        let rocks: Vec<RockInstance> = world
            .asteroids
            .iter()
            .take(MAX_ROCKS)
            .map(|rock| RockInstance {
                pos: rock.pos,
                radius: rock.radius,
                angle: rock.angle,
                // Small enough to stay exact as an f32.
                look: (rock.look >> 16) as f32,
                shape: rock.shape,
            })
            .collect();
        self.queue
            .write_buffer(&self.rock_buffer, 0, bytemuck::cast_slice(&rocks));

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
        let target = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("world pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_bind_group(0, &self.view_bind_group, &[]);
            pass.set_pipeline(&self.stars_pipeline);
            pass.draw(0..3, 0..1);
            pass.set_pipeline(&self.rock_pipeline);
            pass.set_vertex_buffer(0, self.rock_buffer.slice(..));
            pass.draw(0..VERTS_PER_OBJECT, 0..rocks.len() as u32);
            pass.set_pipeline(&self.ship_pipeline);
            pass.set_bind_group(1, &self.ship_bind_group, &[]);
            pass.draw(0..VERTS_PER_OBJECT, 0..1);
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

fn uniform_buffer<T>(device: &wgpu::Device, label: &str) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: size_of::<T>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

/// A pipeline drawing alpha-tested pixel art with `vs` and `fs` from `source`.
fn pipeline(
    device: &wgpu::Device,
    label: &str,
    source: &str,
    bind_group_layouts: &[&wgpu::BindGroupLayout],
    buffers: &[Option<wgpu::VertexBufferLayout>],
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let bind_group_layouts: Vec<_> = bind_group_layouts.iter().copied().map(Some).collect();
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &bind_group_layouts,
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers,
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(format.into())],
        }),
        multiview_mask: None,
        cache: None,
    })
}
