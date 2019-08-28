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
const FULL_CROP: Crop = Crop { x: 0, y: 0, w: WIDTH as u16, h: HEIGHT as u16 };
const PIXEL_FORMAT: PixelFormatEnum = PixelFormatEnum::RGB24;

const WIN_WIDTH:  u32 = (WIDTH  / 2) as u32;
const WIN_HEIGHT: u32 = (HEIGHT / 2) as u32;

#[derive(Copy, Clone, PartialEq, Eq)]
struct Crop {
    x: u16,
    y: u16,
    w: u16,
    h: u16,
}

struct ChannelState {
    crop: Crop,
    full_rect: Rect,
    zoom_rect: Rect,
    server: TcpStream,
}

fn main() {
    let sdl = sdl2::init().unwrap();
    let video_subsystem = sdl.video().unwrap();
    let window = video_subsystem
        .window("Voctozoom", WIDTH as u32, HEIGHT as u32)
//        .resizable()
        .build()
        .unwrap();

    let mut state = [ChannelState {
        crop: FULL_CROP,
        full_rect: Rect::new(0, 0,                 WIN_WIDTH, WIN_HEIGHT),
        zoom_rect: Rect::new(0, WIN_HEIGHT as i32, WIN_WIDTH, WIN_HEIGHT),
        server: TcpStream::connect("127.0.0.1:20000").unwrap(),
        // TODO add second channel
    }];

    let mut canvas : Canvas<Window> = window.into_canvas()
        .present_vsync() //< this means the screen cannot
        // render faster than your display rate (usually 60Hz or 144Hz)
        .build().unwrap();

    for channel in &mut state {
        channel.server.write(b"get_image\n").unwrap();
        let mut frame: [u8; BYTES_PER_FRAME] = [0; BYTES_PER_FRAME];
        channel.server.read_exact(& mut frame).unwrap();

        let surf = Surface::from_data(& mut frame, WIDTH as u32, HEIGHT as u32, 3 * WIDTH as u32, PIXEL_FORMAT).unwrap();

        let tc = canvas.texture_creator();
        let tx = tc.create_texture_from_surface(surf).unwrap();

        canvas.copy(&tx, None, channel.full_rect).unwrap();

        let crop = if channel.crop == FULL_CROP { None } else {
            let c = channel.crop;
            Some(Rect::new(c.x as i32, c.y as i32, c.w as u32, c.h as u32))
        };

        canvas.copy(&tx, crop, channel.zoom_rect).unwrap();
    }

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
