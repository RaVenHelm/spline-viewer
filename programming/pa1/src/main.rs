#[macro_use]
extern crate glium;
#[macro_use]
extern crate imgui;
extern crate imgui_sys;
extern crate cgmath;
extern crate docopt;
extern crate rustc_serialize;
extern crate regex;

mod imgui_support;
mod bezier;
mod camera2d;

use std::ops::{Mul, Add};
use std::fs::File;
use std::path::Path;
use std::io::prelude::*;
use std::io::BufReader;

use glium::{DisplayBuild, Surface, DrawParameters};
use glium::vertex::VertexBuffer;
use glium::index::{NoIndices, PrimitiveType};
use glium::glutin::{self, ElementState, Event, VirtualKeyCode};
use docopt::Docopt;
use regex::Regex;

use imgui_support::ImGuiSupport;
use bezier::Bezier;
use camera2d::Camera2d;

#[derive(Copy, Clone, Debug)]
struct Point {
    pos: [f32; 2],
}
impl Point {
    fn new(x: f32, y: f32) -> Point {
        Point { pos: [x, y] }
    }
}
implement_vertex!(Point, pos);

impl Mul<f32> for Point {
    type Output = Point;
    fn mul(self, rhs: f32) -> Point {
        Point { pos: [self.pos[0] * rhs, self.pos[1] * rhs] }
    }
}
impl Add for Point {
    type Output = Point;
    fn add(self, rhs: Point) -> Point {
        Point { pos: [self.pos[0] + rhs.pos[0], self.pos[1] + rhs.pos[1]] }
    }
}

fn import<P: AsRef<Path>>(path: P) -> Vec<Bezier<Point>> {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => panic!("Failed to open file: {}", e),
    };
    let reader = BufReader::new(file);
    let curve_start = Regex::new("(P|Q), *(\\d+)").unwrap();
    let mut curves = Vec::new();
    let mut points = Vec::new();
    let mut num_curves = 0;
    let mut rational_points = false;
    for line in reader.lines() {
        let l = line.unwrap();
        // Skip empty lines and comments
        if l.is_empty() || l.starts_with("#") {
            println!("skipping comment or empty line '{}'", l);
            continue;
        }
        if num_curves == 0 {
            num_curves = l.parse().unwrap();
            println!("Expecting {} curve(s) from the file", num_curves);
            continue;
        }
        if let Some(caps) = curve_start.captures(&l[..]) {
            // If we had a previous curve we're done parsing it
            if !points.is_empty() {
                curves.push(Bezier::new(points));
                points = Vec::new();
            }

            if caps.at(1) == Some("Q") {
                rational_points = true;
                println!("Expecting {} control points for rational curve #{} in file",
                         caps.at(2).unwrap(), curves.len());
            } else {
                rational_points = false;
                println!("Expecting {} control points for polynomial curve #{} in file",
                         caps.at(2).unwrap(), curves.len());
            }
            continue;
        }
        let coords: Vec<_> = l.split(',').collect();
        assert!(coords.len() >= 2);
        let mut x = coords[0].trim().parse().unwrap();
        let mut y = coords[1].trim().parse().unwrap();
        if rational_points {
            //let w = coords[2].trim().parse().unwrap();
            //x /= w;
            //y /= w;
        }
        points.push(Point::new(x, y));
    }
    // Save the last curve we may have parsed
    if !points.is_empty() {
        curves.push(Bezier::new(points));
    }
    curves
}

const USAGE: &'static str = "
Usage:
    bezier [<file>...]
    bezier (-h | --help)

Options:
    -h, --help      Show this message.
";

#[derive(RustcDecodable)]
struct Args {
    arg_file: Option<Vec<String>>,
}

fn main() {
    let args: Args = Docopt::new(USAGE).and_then(|d| d.decode()).unwrap_or_else(|e| e.exit());
    let mut curves = Vec::new();
    if let Some(files) = args.arg_file {
        for f in files {
            curves = import(f);
        }
    }

    let target_gl_versions = glutin::GlRequest::GlThenGles {
        opengl_version: (3, 3),
        opengles_version: (3, 2),
    };
    let mut width = 1280;
    let mut height = 720;
    let display = glutin::WindowBuilder::new()
        .with_dimensions(width, height)
        .with_gl(target_gl_versions)
        .with_gl_profile(glutin::GlProfile::Core)
        .with_title("CS6670 Programming Assignment 1 - Will Usher")
        .with_vsync()
        .build_glium().unwrap();

    println!("Got OpenGL: {:?}", display.get_opengl_version());
    println!("Got GLSL: {:?}", display.get_supported_glsl_version());

    let mut imgui = ImGuiSupport::init();
    let mut imgui_renderer = imgui::glium_renderer::Renderer::init(&mut imgui.imgui, &display).unwrap();

    let control_points_vbo;
    let step_size = 0.01;
    let t_range = (0.0, 1.0);
    let steps = ((t_range.1 - t_range.0) / step_size) as usize;
    let mut points = Vec::with_capacity(steps);
    if curves.is_empty() {
        // Setup the curve
        let control_points = vec![Point::new(1.0, 0.0), Point::new(1.0, 1.0), Point::new(0.0, 1.0)];
        let curve = Bezier::new(control_points);
        control_points_vbo = VertexBuffer::new(&display, &curve.control_points[..]).unwrap();
        for s in 0..steps + 1 {
            let t = step_size * s as f32 + t_range.0;
            points.push(curve.point(t));
        }
    } else {
        control_points_vbo = VertexBuffer::new(&display, &curves[0].control_points[..]).unwrap();
        // Just draw the first one for now
        for s in 0..steps + 1 {
            let t = step_size * s as f32 + t_range.0;
            points.push(curves[0].point(t));
        }
    }

    let mut camera = Camera2d::new();
    let mut projection = cgmath::ortho(width as f32 / -200.0, width as f32 / 200.0, height as f32 / -200.0,
                                   height as f32 / 200.0, -1.0, -10.0);
    let curve_points_vbo = VertexBuffer::new(&display, &points[..]).unwrap();
    let draw_params = DrawParameters {
        point_size: Some(4.0),
        .. Default::default()
    };
    let shader_program = program!(&display,
        330 => {
            vertex: "
                #version 330 core
                uniform mat4 view;
                uniform mat4 projection;
                in vec2 pos;
                void main(void) {
                    gl_Position = projection * view * vec4(pos, 2.0, 1.0);
                }
                ",
            fragment: "
                #version 330 core
                out vec4 color;
                void main(void) {
                    color = vec4(0.7, 0.7, 0.1, 1);
                }
            "
        },
    ).unwrap();

    'outer: loop {
        for e in display.poll_events() {
            match e {
                glutin::Event::Closed => break 'outer,
                Event::KeyboardInput(state, _, code) => {
                    let pressed = state == ElementState::Pressed;
                    match code {
                        Some(VirtualKeyCode::Escape) if pressed => break 'outer,
                        _ => {}
                    }
                },
                Event::MouseMoved(x, y) if imgui.mouse_pressed.0 && !imgui.mouse_hovering_any_window() => {
                    let fbscale = imgui.imgui.display_framebuffer_scale();
                    let delta = ((x - imgui.mouse_pos.0) as f32 / (fbscale.0 * 100.0),
                                 -(y - imgui.mouse_pos.1) as f32 / (fbscale.1 * 100.0));
                    camera.translate(delta.0, delta.1);
                },
                Event::Resized(w, h) => {
                    width = w;
                    height = h;
                    projection = cgmath::ortho(width as f32 / -200.0, width as f32 / 200.0,
                                               height as f32 / -200.0, height as f32 / 200.0, -1.0, -10.0);
                },
                _ => {}
            }
            imgui.update_event(&e);
            if imgui.mouse_wheel != 0.0 && !imgui.mouse_hovering_any_window() {
                let fbscale = imgui.imgui.display_framebuffer_scale();
                camera.zoom(imgui.mouse_wheel / (fbscale.1 * 10.0));
            }
        }
        imgui.update_mouse();

        let mut target = display.draw();
        target.clear_color(0.2, 0.2, 0.2, 1.0);

        let cam: [[f32; 4]; 4] = camera.get_mat4().into();
        let proj: [[f32; 4]; 4] = projection.into();
        let uniforms = uniform! {
            projection: proj,
            view: cam,
        };

        // Draw the curve
        target.draw(&curve_points_vbo, &NoIndices(PrimitiveType::LineStrip),
                    &shader_program, &uniforms, &draw_params).unwrap();
        // Draw the control points
        target.draw(&control_points_vbo, &NoIndices(PrimitiveType::Points),
                    &shader_program, &uniforms, &draw_params).unwrap();

        let ui = imgui.render_ui(&display);
        ui.window(im_str!("Control Panel"))
            .size((300.0, 100.0), imgui::ImGuiSetCond_FirstUseEver)
            .build(|| {
                let fps = ui.framerate();
                let frame_time = 1000.0 / fps;
                let gl_version = display.get_opengl_version();
                let glsl_version = display.get_supported_glsl_version();
                ui.text(im_str!("Framerate: {:.3} FPS ({:.3} ms)", fps, frame_time));
                ui.text(im_str!("OpenGL Version: {}.{}", gl_version.1, gl_version.2));
                ui.text(im_str!("GLSL Version: {}.{}", glsl_version.1, glsl_version.2));
            });
        imgui_renderer.render(&mut target, ui).unwrap();

        target.finish().unwrap();
    }
}

