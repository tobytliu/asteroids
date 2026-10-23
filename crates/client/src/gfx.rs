use std::marker::PhantomData;

use bytemuck::{Pod, Zeroable};
use web_sys::HtmlCanvasElement;
use wgpu::util::DeviceExt;

use crate::camera::View;
use crate::copies::Visible;
use crate::effects::Effects;
use crate::{PIXEL, sprite};

/// Shared by every pipeline (group 0). Matches `View` in common.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ViewUniform {
    /// The view's size in CSS px.
    size: [f32; 2],
    /// Canvas backing-store size in physical px.
    fb_size: [f32; 2],
    /// Where the star layer is scrolled to, in px.
    star_offset: [f32; 2],
    /// Seconds, for animation.
    time: f32,
    /// Screen px per world px.
    zoom: f32,
}

/// One ship, as a per-instance vertex. Matches `ShipInstance` in ship.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ShipInstance {
    /// Relative to the view's center.
    pos: [f32; 2],
    reach: f32,
    angle: f32,
    /// Engine flame intensity, 0 (off) to 1 (full), for its engine pixel's
    /// glow.
    flame: f32,
}

/// One fire's plume, as a per-instance vertex. Matches `FireInstance` in
/// fire.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct FireInstance {
    /// The burner's center, relative to the view's center.
    pos: [f32; 2],
    /// The nozzle, in px in the burner's frame.
    offset: [f32; 2],
    /// Burner's angle, the fire's direction from its heading, intensity, and
    /// how far the tail bends per art pixel.
    motion: [f32; 4],
    /// Longest flame, how wide it billows, the most its tail swings, and a
    /// flicker seed.
    style: [f32; 4],
}

/// One particle, as a per-instance vertex. Matches `ParticleInstance` in
/// particle.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ParticleInstance {
    /// Relative to the view's center.
    pos: [f32; 2],
    /// How hot it is now, and a twinkle seed.
    heat_seed: [f32; 2],
}

/// One asteroid, as a per-instance vertex. Matches `Rock` in asteroid.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct RockInstance {
    /// Relative to the view's center.
    pos: [f32; 2],
    radius: f32,
    angle: f32,
    look: f32,
    /// The rock's outline from the sim, so what's drawn is what collides.
    shape: [f32; sim::SHAPE_POINTS],
    /// The rock's `sim::Class`, which picks how its surface is drawn.
    class: u32,
}

/// Each object is drawn as one square. Must match `VERTS_PER_OBJECT` in
/// common.wgsl.
const VERTS_PER_OBJECT: u32 = 6;
/// How far the ship's sprite can reach from its center at any rotation, in
/// px, with a little margin. Its flame is drawn separately.
const SHIP_REACH: f32 = (sprite::SHIP.len() as f32 / 2.0 * 1.42 + 1.0) * PIXEL;
/// How fast the star layer scrolls compared to the map. Below 1, so the stars
/// look far behind it.
const PARALLAX: f64 = 0.5;
/// The star pattern repeats this far apart, in px. Must match `STAR_PERIOD`
/// × `PIXEL` in stars.wgsl.
const STAR_PERIOD_PX: f64 = 4096.0 * 3.0;

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
    ship_bind_group: wgpu::BindGroup,
    ships: Instances<ShipInstance>,
    rock_pipeline: wgpu::RenderPipeline,
    rocks: Instances<RockInstance>,
    fire_pipeline: wgpu::RenderPipeline,
    fires: Instances<FireInstance>,
    particle_pipeline: wgpu::RenderPipeline,
    particles: Instances<ParticleInstance>,
    /// Animation clock, in seconds.
    time: f32,
    /// Fire, sparks and anything else that's only for show.
    effects: Effects,
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
            concat!(
                include_str!("common.wgsl"),
                include_str!("heat.wgsl"),
                include_str!("ship.wgsl")
            ),
            &[&view_layout, &ship_layout],
            &[Some(wgpu::VertexBufferLayout {
                array_stride: size_of::<ShipInstance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![
                    0 => Float32x2,
                    1 => Float32,
                    2 => Float32,
                    3 => Float32,
                ],
            })],
            config.format,
        );
        let fire_pipeline = pipeline(
            &device,
            "fire",
            concat!(
                include_str!("common.wgsl"),
                include_str!("heat.wgsl"),
                include_str!("fire.wgsl")
            ),
            &[&view_layout],
            &[Some(wgpu::VertexBufferLayout {
                array_stride: size_of::<FireInstance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![
                    0 => Float32x2,
                    1 => Float32x2,
                    2 => Float32x4,
                    3 => Float32x4,
                ],
            })],
            config.format,
        );
        let particle_pipeline = pipeline(
            &device,
            "particle",
            concat!(
                include_str!("common.wgsl"),
                include_str!("heat.wgsl"),
                include_str!("particle.wgsl")
            ),
            &[&view_layout],
            &[Some(wgpu::VertexBufferLayout {
                array_stride: size_of::<ParticleInstance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2],
            })],
            config.format,
        );

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
                    // The 32-point outline, as eight vec4s.
                    4 => Float32x4,
                    5 => Float32x4,
                    6 => Float32x4,
                    7 => Float32x4,
                    8 => Float32x4,
                    9 => Float32x4,
                    10 => Float32x4,
                    11 => Float32x4,
                    12 => Uint32,
                ],
            })],
            config.format,
        );

        let ships = Instances::new(&device, "ship instances");
        let rocks = Instances::new(&device, "rock instances");
        let fires = Instances::new(&device, "fire instances");
        let particles = Instances::new(&device, "particle instances");
        Self {
            canvas,
            surface,
            device,
            queue,
            config,
            view_buffer,
            view_bind_group,
            stars_pipeline,
            ships,
            ship_pipeline,
            ship_bind_group,
            rocks,
            rock_pipeline,
            fires,
            fire_pipeline,
            particles,
            particle_pipeline,
            time: 0.0,
            effects: Effects::default(),
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

    /// The view's size: the canvas's size in CSS px.
    pub fn view_size(&self) -> [f32; 2] {
        [
            self.canvas.client_width().max(1) as f32,
            self.canvas.client_height().max(1) as f32,
        ]
    }

    /// Draw what `view` shows of `world`. `dt` (seconds since the last frame)
    /// only drives animation.
    pub fn render(&mut self, world: &sim::Snapshot, view: View, dt: f32) {
        // Wrap the clock so it never grows large enough to lose f32 precision.
        self.time = (self.time + dt) % 3600.0;
        self.effects.update(world, dt);

        let size = self.view_size();
        let fb_size = [self.config.width as f32, self.config.height as f32];
        let device_px_per_css = (fb_size[0] / size[0]) as f64;
        let view = view.snapped(device_px_per_css);
        // The star layer scrolls with the camera's continuous center, never
        // the wrapped one, so it doesn't jump at the seam. Rounded to device
        // pixels like the view, then wrapped by the star pattern's period so
        // it stays small enough for the GPU's f32 maths.
        let star_scale = device_px_per_css * view.zoom as f64;
        let star_offset = view.center.map(|c| {
            ((c * PARALLAX * star_scale).round() / star_scale).rem_euclid(STAR_PERIOD_PX) as f32
        });
        let uniform = ViewUniform {
            size,
            fb_size,
            star_offset,
            time: self.time,
            zoom: view.zoom,
        };
        self.queue
            .write_buffer(&self.view_buffer, 0, bytemuck::bytes_of(&uniform));

        let visible = Visible::new(&view, size, world.size);
        let ships: Vec<ShipInstance> = world
            .ships
            .iter()
            .flat_map(|ship| {
                // Its engine is its first mount.
                let flame = self.effects.fires.get(ship.id, 0).map_or(0.0, |f| f.intensity);
                visible
                    .copies(ship.body.pos, SHIP_REACH)
                    .map(move |pos| ShipInstance {
                        pos,
                        reach: SHIP_REACH,
                        angle: ship.body.angle,
                        flame,
                    })
            })
            .collect();
        let rocks: Vec<RockInstance> = world
            .rocks
            .iter()
            .flat_map(|rock| {
                let shape = &rock.asteroid;
                // The square a rock is drawn in reaches one art pixel past its
                // outline (see asteroid.wgsl).
                visible
                    .copies(rock.body.pos, shape.radius + PIXEL)
                    .map(|pos| RockInstance {
                        pos,
                        radius: shape.radius,
                        angle: rock.body.angle,
                        // Small enough to stay exact as an f32.
                        look: (shape.look >> 16) as f32,
                        shape: shape.shape,
                        class: shape.class as u32,
                    })
            })
            .collect();
        let fires: Vec<FireInstance> = self
            .effects
            .fires
            .iter()
            .filter(|fire| fire.intensity >= 0.02)
            .flat_map(|fire| {
                let plume = &fire.mount.style.plume;
                // Matches the reach in fire.wgsl.
                let offset = fire.mount.offset;
                let reach =
                    offset[0].hypot(offset[1]) + (plume.len * 1.25 + plume.width + 2.0) * PIXEL;
                let motion = [fire.pose.angle, fire.mount.dir, fire.intensity, fire.bend()];
                let style = [plume.len, plume.width, plume.max_swing, fire.seed];
                visible
                    .copies(fire.pose.pos, reach)
                    .map(move |pos| FireInstance {
                        pos,
                        offset,
                        motion,
                        style,
                    })
            })
            .collect();
        let particles: Vec<ParticleInstance> = self
            .effects
            .particles
            .iter()
            .flat_map(|p| {
                let heat_seed = [p.heat_now(), p.seed];
                visible
                    .copies(p.pos, PIXEL)
                    .map(move |pos| ParticleInstance { pos, heat_seed })
            })
            .collect();
        self.ships.write(&self.device, &self.queue, &ships);
        self.rocks.write(&self.device, &self.queue, &rocks);
        self.fires.write(&self.device, &self.queue, &fires);
        self.particles.write(&self.device, &self.queue, &particles);

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
            pass.set_vertex_buffer(0, self.rocks.buffer.slice(..));
            pass.draw(0..VERTS_PER_OBJECT, 0..rocks.len() as u32);
            // Sparks under the flames, and both under the ships. Often there
            // are none, and WebGPU warns about empty draws.
            if !particles.is_empty() {
                pass.set_pipeline(&self.particle_pipeline);
                pass.set_vertex_buffer(0, self.particles.buffer.slice(..));
                pass.draw(0..VERTS_PER_OBJECT, 0..particles.len() as u32);
            }
            if !fires.is_empty() {
                pass.set_pipeline(&self.fire_pipeline);
                pass.set_vertex_buffer(0, self.fires.buffer.slice(..));
                pass.draw(0..VERTS_PER_OBJECT, 0..fires.len() as u32);
            }
            pass.set_pipeline(&self.ship_pipeline);
            pass.set_bind_group(1, &self.ship_bind_group, &[]);
            pass.set_vertex_buffer(0, self.ships.buffer.slice(..));
            pass.draw(0..VERTS_PER_OBJECT, 0..ships.len() as u32);
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
    }
}

/// A vertex buffer of per-instance `T`s that grows when more are drawn than
/// fit.
struct Instances<T> {
    label: &'static str,
    buffer: wgpu::Buffer,
    capacity: usize,
    _instance: PhantomData<T>,
}

impl<T: Pod> Instances<T> {
    const INITIAL_CAPACITY: usize = 64;

    fn new(device: &wgpu::Device, label: &'static str) -> Self {
        Self {
            label,
            buffer: instance_buffer::<T>(device, label, Self::INITIAL_CAPACITY),
            capacity: Self::INITIAL_CAPACITY,
            _instance: PhantomData,
        }
    }

    fn write(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, instances: &[T]) {
        if instances.len() > self.capacity {
            self.capacity = instances.len().next_power_of_two();
            self.buffer = instance_buffer::<T>(device, self.label, self.capacity);
        }
        if !instances.is_empty() {
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(instances));
        }
    }
}

fn instance_buffer<T>(device: &wgpu::Device, label: &str, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: (capacity * size_of::<T>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
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
