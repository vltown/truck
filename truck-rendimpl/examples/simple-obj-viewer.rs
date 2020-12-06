use std::path::PathBuf;
use truck_platform::*;
use truck_polymesh::*;
use truck_rendimpl::*;
use wgpu::*;
use winit::{dpi::*, event::*, event_loop::ControlFlow};
mod app;
use app::*;

struct MyApp {
    scene: Scene,
    rotate_flag: bool,
    prev_cursor: Option<Vector2>,
    path: Option<PathBuf>,
    light_changed: Option<std::time::Instant>,
    camera_changed: Option<std::time::Instant>,
}

impl MyApp {
    fn create_camera() -> Camera {
        let matrix = Matrix4::look_at(
            Point3::new(1.0, 1.0, 1.0),
            Point3::origin(),
            Vector3::unit_y(),
        );
        Camera::perspective_camera(
            matrix.invert().unwrap(),
            Rad(std::f64::consts::PI / 4.0),
            0.1,
            40.0,
        )
    }
    fn set_normals(mesh: PolygonMesh) -> PolygonMesh {
        match mesh.normals.is_empty() {
            false => mesh,
            true => {
                let mut mesh_handler = MeshHandler::new(mesh);
                mesh_handler
                    .put_together_same_attrs()
                    .add_smooth_normal(0.5);
                mesh_handler.into()
            }
        }
    }

    fn load_obj<P: AsRef<std::path::Path>>(&mut self, path: P) {
        let scene = &mut self.scene;
        scene.clear_objects();
        let file = std::fs::File::open(path).unwrap();
        let mesh = truck_polymesh::obj::read(file).unwrap();
        let mesh = MyApp::set_normals(mesh);
        let bdd_box = mesh.bounding_box();
        let (size, center) = (bdd_box.size(), bdd_box.center());
        let mat = Matrix4::from_translation(center.to_vec()) * Matrix4::from_scale(size);
        let inst_desc = InstanceDescriptor {
            matrix: mat.invert().unwrap(),
            material: Material {
                albedo: Vector4::new(0.75, 0.75, 0.75, 1.0),
                reflectance: 0.9,
                roughness: 0.1,
                ambient_ratio: 0.02,
            },
            ..Default::default()
        };
        let mut mesh = scene.create_instance(&mesh, &inst_desc);
        scene.add_object(&mut mesh);
    }
}

impl App for MyApp {
    fn init(handler: &DeviceHandler) -> MyApp {
        let scene_desc = SceneDescriptor {
            background: Color::BLACK,
            camera: MyApp::create_camera(),
            lights: vec![Light {
                position: Point3::new(1.0, 1.0, 1.0),
                color: Vector3::new(1.0, 1.0, 1.0),
                light_type: LightType::Point,
            }],
        };
        MyApp {
            scene: Scene::new(handler.clone(), &scene_desc),
            rotate_flag: false,
            prev_cursor: None,
            path: None,
            camera_changed: None,
            light_changed: None,
        }
    }

    fn app_title<'a>() -> Option<&'a str> { Some("simple obj viewer") }

    fn depth_stencil_attachment_descriptor<'a>(
        &'a self,
    ) -> Option<RenderPassDepthStencilAttachmentDescriptor<'a>> {
        Some(self.scene.depth_stencil_attachment_descriptor())
    }

    fn dropped_file(&mut self, path: std::path::PathBuf) -> ControlFlow {
        self.path = Some(path);
        Self::default_control_flow()
    }

    fn mouse_input(&mut self, state: ElementState, button: MouseButton) -> ControlFlow {
        match button {
            MouseButton::Left => {
                self.rotate_flag = state == ElementState::Pressed;
                if !self.rotate_flag {
                    self.prev_cursor = None;
                }
            }
            MouseButton::Right => {
                let (light, camera) = {
                    let desc = self.scene.descriptor_mut();
                    (&mut desc.lights[0], &desc.camera)
                };
                match light.light_type {
                    LightType::Point => {
                        light.position = camera.position();
                    }
                    LightType::Uniform => {
                        light.position = camera.position();
                        let strength = light.position.to_vec().magnitude();
                        light.position /= strength;
                    }
                }
            }
            _ => {}
        }
        Self::default_control_flow()
    }
    fn mouse_wheel(&mut self, delta: MouseScrollDelta, _: TouchPhase) -> ControlFlow {
        match delta {
            MouseScrollDelta::LineDelta(_, y) => {
                let camera = &mut self.scene.descriptor_mut().camera;
                let trans_vec = camera.eye_direction() * 0.2 * y as f64;
                camera.matrix = Matrix4::from_translation(trans_vec) * camera.matrix;
            }
            MouseScrollDelta::PixelDelta(_) => {}
        };
        Self::default_control_flow()
    }

    fn cursor_moved(&mut self, position: PhysicalPosition<f64>) -> ControlFlow {
        if self.rotate_flag {
            let matrix = &mut self.scene.descriptor_mut().camera.matrix;
            let position = Vector2::new(position.x, position.y);
            if let Some(ref prev_position) = self.prev_cursor {
                let dir2d = &position - prev_position;
                let mut axis = dir2d[1] * matrix[0].truncate();
                axis += dir2d[0] * &matrix[1].truncate();
                axis /= axis.magnitude();
                let angle = dir2d.magnitude() * 0.01;
                let mat = Matrix4::from_axis_angle(axis, Rad(angle));
                *matrix = mat.invert().unwrap() * *matrix;
            }
            self.prev_cursor = Some(position);
        }
        Self::default_control_flow()
    }
    fn keyboard_input(&mut self, input: KeyboardInput, _: bool) -> ControlFlow {
        let keycode = match input.virtual_keycode {
            Some(keycode) => keycode,
            None => return Self::default_control_flow(),
        };
        match keycode {
            VirtualKeyCode::P => {
                if let Some(ref instant) = self.camera_changed {
                    let time = instant.elapsed().as_secs_f64();
                    if time < 0.2 {
                        return Self::default_control_flow();
                    }
                }
                let camera = &mut self.scene.descriptor_mut().camera;
                self.camera_changed = Some(std::time::Instant::now());
                *camera = match camera.projection_type() {
                    ProjectionType::Parallel => Camera::perspective_camera(
                        camera.matrix,
                        Rad(std::f64::consts::PI / 4.0),
                        0.1,
                        40.0,
                    ),
                    ProjectionType::Perspective => {
                        Camera::parallel_camera(camera.matrix, 1.0, 0.1, 100.0)
                    }
                }
            }
            VirtualKeyCode::L => {
                if let Some(ref instant) = self.light_changed {
                    let time = instant.elapsed().as_secs_f64();
                    if time < 0.2 {
                        return Self::default_control_flow();
                    }
                }
                let (light, camera) = {
                    let desc = self.scene.descriptor_mut();
                    (&mut desc.lights[0], &desc.camera)
                };
                self.light_changed = Some(std::time::Instant::now());
                *light = match light.light_type {
                    LightType::Point => {
                        let mut vec = camera.position();
                        vec /= vec.to_vec().magnitude();
                        Light {
                            position: vec,
                            color: Vector3::new(1.0, 1.0, 1.0),
                            light_type: LightType::Uniform,
                        }
                    }
                    LightType::Uniform => {
                        let position = camera.position();
                        Light {
                            position,
                            color: Vector3::new(1.0, 1.0, 1.0),
                            light_type: LightType::Point,
                        }
                    }
                };
            }
            _ => {}
        }
        Self::default_control_flow()
    }

    fn update(&mut self, _: &DeviceHandler) {
        if let Some(path) = self.path.take() {
            self.load_obj(path);
        }
        self.scene.prepare_render();
    }

    fn render(&self, frame: &SwapChainFrame) { self.scene.render_scene(&frame.output.view); }
}

fn main() { MyApp::run(); }
