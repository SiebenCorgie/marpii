use std::time::Instant;
use crate::Ubo;
use glam::{Quat, Vec3, Mat4};
use winit::{window::Window, event::{Event, WindowEvent, KeyboardInput, ElementState, VirtualKeyCode, DeviceEvent}};

pub struct Camera{
    pub location: Vec3,
    pub rotation: Quat,

    //current worldspace velocity
    velocity: Vec3,
    //target velocity local to camera
    target_velocity: Vec3,

    upd: Instant,
}

impl Camera{
    const MOUSE_SPEED: f32 = 0.001;
    const SPEED: f32 = 10.0;
    const BREAK_DIVISOR: f32 = 10_000.0;
    const STOP_THREASOLD: f32 = 0.1;
    pub fn on_event(&mut self, event: &Event<()>){
        match event{
            Event::DeviceEvent {
                event: DeviceEvent::MouseMotion { delta: (x, y) },
                ..
            } => {
                let right = self.rotation.mul_vec3(Vec3::new(1.0, 0.0, 0.0));
                    let rot_yaw = Quat::from_rotation_y(*x as f32 * Self::MOUSE_SPEED);
                    let rot_pitch = Quat::from_axis_angle(right, *y as f32 * Self::MOUSE_SPEED);

                    let to_add = rot_yaw * rot_pitch;
                    self.rotation = to_add * self.rotation;
            }
            Event::WindowEvent { event: WindowEvent::KeyboardInput { input: KeyboardInput{state, virtual_keycode: Some(key_code), ..}, .. }, .. } => {

                let speed = match state{
                    ElementState::Pressed => Self::SPEED,
                    _ => 0.0
                };

                match key_code {
                    VirtualKeyCode::A => self.target_velocity.x = -speed,
                    VirtualKeyCode::D => self.target_velocity.x = speed,
                    VirtualKeyCode::E => self.target_velocity.y = -speed,
                    VirtualKeyCode::Q => self.target_velocity.y = speed,
                    VirtualKeyCode::S => self.target_velocity.z = -speed,
                    VirtualKeyCode::W => self.target_velocity.z = speed,
                    _ => {}
                }
            },
            _ => {}
        }
    }

    pub fn tick(&mut self){
        let delta = self.upd.elapsed().as_secs_f32();
        self.upd = Instant::now();

        //transform into rotation based offset and add
        self.velocity /= Self::BREAK_DIVISOR * delta; // decrease last velocity
        //add on all axis that are active
        self.velocity = (self.velocity + self.target_velocity).clamp(Vec3::splat(-Self::SPEED), Vec3::splat(Self::SPEED));
        //finaly null axis that are really slow
        for i in 0..3{
            if self.velocity[i].abs() < Self::STOP_THREASOLD && self.velocity[i].abs() > f32::EPSILON{
                self.velocity[i] = 0.0;
            }
        }
        //now add world space offset
        self.location += self.rotation.mul_vec3(self.velocity) * delta;
        println!("{}", self.location);
    }

    pub fn to_ubo(&self, window: &Window) -> Ubo{

        let aspect = window.inner_size().width as f32 / window.inner_size().height as f32;
        let transform = Mat4::from_rotation_translation(self.rotation, self.location).inverse();
        let perspective = Mat4::perspective_lh(90.0f32.to_radians(), aspect, 0.001, 1000.0);

        Ubo { model_view: transform.to_cols_array_2d(), perspective: perspective.to_cols_array_2d() }
    }
}

impl Default for Camera{
    fn default() -> Self {
        Camera {
            location: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            velocity: Vec3::ZERO,
            target_velocity: Vec3::ZERO,
            upd: Instant::now(),
        }
    }
}
