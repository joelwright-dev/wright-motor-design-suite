//! 3D viewport: an orbit camera and a wgpu mesh renderer embedded in egui.

use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use eframe::egui_wgpu;
use glam::{DVec3, Mat4, Vec3};
use wgpu::util::DeviceExt;
use wmds_geom::Mesh;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    light_dir: [f32; 4],
    color: [f32; 4],
    camera_pos: [f32; 4],
}

/// CPU-side mesh ready for upload, shared between frames by `Arc`.
pub struct GpuMeshData {
    pub version: u64,
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
}

impl GpuMeshData {
    /// Build from a `wmds_geom::Mesh`. Uses the mesh normals when present and consistent,
    /// otherwise flat-shades by duplicating vertices per triangle.
    pub fn from_mesh(mesh: &Mesh, version: u64) -> Self {
        if !mesh.normals.is_empty() && mesh.normals.len() == mesh.positions.len() {
            let vertices = mesh
                .positions
                .iter()
                .zip(&mesh.normals)
                .map(|(p, n)| Vertex {
                    position: [p[0] as f32, p[1] as f32, p[2] as f32],
                    normal: [n[0] as f32, n[1] as f32, n[2] as f32],
                })
                .collect();
            let indices = mesh
                .triangles
                .iter()
                .flat_map(|t| t.iter().copied())
                .collect();
            return GpuMeshData {
                version,
                vertices,
                indices,
            };
        }
        let mut vertices = Vec::with_capacity(mesh.triangles.len() * 3);
        let mut indices = Vec::with_capacity(mesh.triangles.len() * 3);
        for t in &mesh.triangles {
            let p: Vec<DVec3> = t
                .iter()
                .map(|&i| {
                    let q = mesh.positions[i as usize];
                    DVec3::new(q[0], q[1], q[2])
                })
                .collect();
            let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
            for q in p {
                indices.push(vertices.len() as u32);
                vertices.push(Vertex {
                    position: [q.x as f32, q.y as f32, q.z as f32],
                    normal: [n.x as f32, n.y as f32, n.z as f32],
                });
            }
        }
        GpuMeshData {
            version,
            vertices,
            indices,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }
}

/// Orbit camera. Z is up (ISO 8855 vehicle axes).
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub target: Vec3,
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub fov_y: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Camera {
            target: Vec3::ZERO,
            distance: 1.0,
            yaw: -0.8,
            pitch: 0.5,
            fov_y: 40f32.to_radians(),
        }
    }
}

impl Camera {
    pub fn eye(&self) -> Vec3 {
        let cp = self.pitch.cos();
        let dir = Vec3::new(cp * self.yaw.cos(), cp * self.yaw.sin(), self.pitch.sin());
        self.target + dir * self.distance
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let near = (self.distance * 0.01).max(1e-4);
        let far = self.distance * 100.0;
        let proj = Mat4::perspective_rh(self.fov_y, aspect.max(1e-3), near, far);
        let view = Mat4::look_at_rh(self.eye(), self.target, Vec3::Z);
        proj * view
    }

    /// Frame a bounding box.
    pub fn frame(&mut self, lo: Vec3, hi: Vec3) {
        self.target = (lo + hi) * 0.5;
        let radius = ((hi - lo).length() * 0.5).max(1e-3);
        self.distance = radius / (self.fov_y * 0.5).sin() * 1.1;
    }

    /// Point the camera at one of the standard engineering views.
    ///
    /// Named views matter more here than in a general 3D viewer. Most of what goes wrong with a
    /// modular vehicle, such as a handed part reaching the wrong way or a component fouling a
    /// rail, is obvious from directly above or directly ahead and nearly invisible from a three
    /// quarter view. Vehicle axes are x rearward, y to the left, z up.
    pub fn set_view(&mut self, name: &str) -> bool {
        use std::f32::consts::{FRAC_PI_2, PI};
        let (yaw, pitch) = match name {
            // Looking straight down. Yaw puts vehicle +x to the right of the image.
            "top" => (PI, FRAC_PI_2 - 0.0001),
            "bottom" => (PI, -FRAC_PI_2 + 0.0001),
            // From the left of the vehicle, which is +y.
            "left" => (FRAC_PI_2, 0.0),
            "right" => (-FRAC_PI_2, 0.0),
            // From in front, which is -x.
            "front" => (PI, 0.0),
            "rear" => (0.0, 0.0),
            "iso" => (-0.8, 0.5),
            _ => return false,
        };
        self.yaw = yaw;
        self.pitch = pitch;
        true
    }

    /// The names `set_view` accepts, for a menu or a command line.
    pub const VIEWS: [&'static str; 7] = ["iso", "top", "bottom", "front", "rear", "left", "right"];

    /// Project a world point to viewport pixel coordinates within `rect`.
    pub fn project(&self, rect: egui::Rect, p: Vec3) -> Option<egui::Pos2> {
        let vp = self.view_proj(rect.aspect_ratio());
        let clip = vp * p.extend(1.0);
        if clip.w <= 0.0 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        Some(egui::pos2(
            rect.left() + (ndc.x + 1.0) * 0.5 * rect.width(),
            rect.top() + (1.0 - ndc.y) * 0.5 * rect.height(),
        ))
    }
}

/// GPU resources kept in egui-wgpu's callback resource map.
struct RenderResources {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    vertex_buffer: Option<wgpu::Buffer>,
    index_buffer: Option<wgpu::Buffer>,
    index_count: u32,
    uploaded_version: u64,
}

/// Call once at app creation.
pub fn init(cc: &eframe::CreationContext<'_>, depth_format: Option<wgpu::TextureFormat>) -> bool {
    let Some(rs) = cc.wgpu_render_state.as_ref() else {
        return false;
    };
    let device = &rs.device;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("wmds-viewport"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
    });
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("wmds-viewport"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: std::num::NonZeroU64::new(std::mem::size_of::<Uniforms>() as u64),
            },
            count: None,
        }],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("wmds-viewport"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("wmds-viewport"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(rs.target_format.into())],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: depth_format.map(|format| wgpu::DepthStencilState {
            format,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });
    let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("wmds-viewport-uniforms"),
        contents: bytemuck::bytes_of(&Uniforms {
            view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            light_dir: [0.4, 0.3, 0.85, 0.0],
            color: [0.75, 0.78, 0.82, 1.0],
            camera_pos: [0.0; 4],
        }),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("wmds-viewport"),
        layout: &bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform_buffer.as_entire_binding(),
        }],
    });
    rs.renderer
        .write()
        .callback_resources
        .insert(RenderResources {
            pipeline,
            bind_group,
            uniform_buffer,
            vertex_buffer: None,
            index_buffer: None,
            index_count: 0,
            uploaded_version: 0,
        });
    true
}

/// One frame's paint request.
pub struct ViewportCallback {
    pub mesh: Option<Arc<GpuMeshData>>,
    pub camera: Camera,
    pub aspect: f32,
    pub color: [f32; 4],
}

impl egui_wgpu::CallbackTrait for ViewportCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(r) = resources.get_mut::<RenderResources>() else {
            return Vec::new();
        };
        let eye = self.camera.eye();
        let u = Uniforms {
            view_proj: self.camera.view_proj(self.aspect).to_cols_array_2d(),
            light_dir: [0.4, 0.3, 0.85, 0.0],
            color: self.color,
            camera_pos: [eye.x, eye.y, eye.z, 1.0],
        };
        queue.write_buffer(&r.uniform_buffer, 0, bytemuck::bytes_of(&u));
        match &self.mesh {
            Some(m) if m.version != r.uploaded_version => {
                if m.is_empty() {
                    r.vertex_buffer = None;
                    r.index_buffer = None;
                    r.index_count = 0;
                } else {
                    r.vertex_buffer = Some(device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("wmds-viewport-vertices"),
                            contents: bytemuck::cast_slice(&m.vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    ));
                    r.index_buffer = Some(device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("wmds-viewport-indices"),
                            contents: bytemuck::cast_slice(&m.indices),
                            usage: wgpu::BufferUsages::INDEX,
                        },
                    ));
                    r.index_count = m.indices.len() as u32;
                }
                r.uploaded_version = m.version;
            }
            None => {
                r.vertex_buffer = None;
                r.index_buffer = None;
                r.index_count = 0;
                r.uploaded_version = 0;
            }
            _ => {}
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        let Some(r) = resources.get::<RenderResources>() else {
            return;
        };
        let (Some(vb), Some(ib)) = (&r.vertex_buffer, &r.index_buffer) else {
            return;
        };
        pass.set_pipeline(&r.pipeline);
        pass.set_bind_group(0, &r.bind_group, &[]);
        pass.set_vertex_buffer(0, vb.slice(..));
        pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..r.index_count, 0, 0..1);
    }
}

/// Apply mouse input from `response` to the camera: drag orbits, shift-drag pans, wheel zooms.
pub fn handle_input(camera: &mut Camera, ui: &egui::Ui, response: &egui::Response) {
    if response.dragged() {
        let d = response.drag_motion();
        let shift = ui.input(|i| i.modifiers.shift);
        if shift || response.dragged_by(egui::PointerButton::Middle) {
            // Pan in the camera plane.
            let eye = camera.eye();
            let forward = (camera.target - eye).normalize_or_zero();
            let right = forward.cross(Vec3::Z).normalize_or_zero();
            let up = right.cross(forward).normalize_or_zero();
            let scale = camera.distance * 0.0015;
            camera.target += (-right * d.x + up * d.y) * scale;
        } else {
            camera.yaw -= d.x * 0.008;
            camera.pitch = (camera.pitch + d.y * 0.008).clamp(-1.5, 1.5);
        }
    }
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            camera.distance = (camera.distance * (1.0 - scroll * 0.002)).clamp(1e-3, 1e4);
        }
    }
}
