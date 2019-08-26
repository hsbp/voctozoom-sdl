extern crate sdl2;

use std::net::{TcpStream};
use std::io::{Read, Write};

use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;
use sdl2::surface::Surface;

const WIDTH: usize = 1280;
const HEIGHT: usize = 720;
const BYTES_PER_PIXEL: usize = 3;
const BYTES_PER_FRAME: usize = WIDTH * HEIGHT * BYTES_PER_PIXEL;

fn main() {
    let sdl = sdl2::init().unwrap();
    let video_subsystem = sdl.video().unwrap();
    let window = video_subsystem
        .window("Voctozoom", WIDTH as u32, HEIGHT as u32)
//        .resizable()
        .build()
        .unwrap();

    let mut ts = TcpStream::connect("127.0.0.1:20000").unwrap();
    ts.write(b"get_image\n").unwrap();
    let mut frame: [u8; BYTES_PER_FRAME] = [0; BYTES_PER_FRAME];
    ts.read_exact(& mut frame).unwrap();

    let mut canvas : Canvas<Window> = window.into_canvas()
        .present_vsync() //< this means the screen cannot
        // render faster than your display rate (usually 60Hz or 144Hz)
        .build().unwrap();

    let surf = Surface::from_data(& mut frame, WIDTH as u32, HEIGHT as u32, 3 * WIDTH as u32, PixelFormatEnum::RGB24).unwrap();
    let tc = canvas.texture_creator();
    let tx = tc.create_texture_from_surface(surf).unwrap();
    canvas.copy(&tx, None, None).unwrap();

    // XXX eprintln!("{:?}", canvas.output_size());

    // However the canvas has not been updated to the window yet,
    // everything has been processed to an internal buffer,
    // but if we want our buffer to be displayed on the window,
    // we need to call `present`. We need to call this everytime
    // we want to render a new frame on the window.
    canvas.present();

    let mut event_pump = sdl.event_pump().unwrap();
    'main: loop {
        for event in event_pump.poll_iter() {
            match event {
                sdl2::event::Event::Quit {..} => break 'main,
                _ => {},
            }
        }
    }
}
