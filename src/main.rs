extern crate sdl2;

use std::net::{TcpStream};
use std::io::{Read, Write};
use std::io::{BufRead,BufReader};

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

const ZOOM_FACTOR: f32 = 0.9;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
struct Crop {
    x: u16,
    y: u16,
    w: u16,
    h: u16,
}

impl Into<Rect> for Crop {
    fn into(self) -> Rect {
        Rect::new(self.x as i32, self.y as i32, self.w as u32, self.h as u32)
    }
}

struct ChannelState {
    crop: Crop,
    full_rect: Rect,
    zoom_rect: Rect,
    server: TcpStream,
    frame: Vec<u8>,
}

impl ChannelState {
    fn set_crop(& mut self, nr: Rect) -> bool {
        eprintln!("nr = {:?}, w = {}, h = {}", nr, nr.width(), nr.height());
        let new_crop = Crop { x: nr.left() as u16, y: nr.top() as u16,
        w: nr.width() as u16, h: nr.height() as u16 };
        eprintln!("{:?} =?= {:?}", new_crop, self.crop);
        if new_crop == self.crop { return false; }

        let line = self.text_cmd(format!("zoom_to {}x{}+{}+{}\n", nr.width(), nr.height(), nr.left(), nr.top()));
        eprintln!("{:?}", line);
        if line == "OK\n" {
            self.crop = new_crop;
            return true;
        } else {
            return false;
        }
    }

    fn text_cmd(& mut self, cmd: String) -> String {
        self.server.write(&cmd.into_bytes()).unwrap();
        let mut br = BufReader::new(&self.server);
        let mut line = String::new();
        br.read_line(&mut line).unwrap();
        line
    }
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
        frame: vec![0; BYTES_PER_FRAME],
        // TODO add second channel
    }];

    sanity_check(& mut state);

    let mut canvas : Canvas<Window> = window.into_canvas()
        .present_vsync() //< this means the screen cannot
        // render faster than your display rate (usually 60Hz or 144Hz)
        .build().unwrap();

    get_video(& mut state);
    update_video(& mut canvas, & mut state);

    let mut event_pump = sdl.event_pump().unwrap();
    let mut mouse_start_pos: Option<(i32, i32)> = None;
    let mut mouse_pos: (i32, i32) = (0, 0);

    'main: loop {
        for event in event_pump.wait_timeout_iter(200) {
            match event {
                sdl2::event::Event::Quit {..} => break 'main,
                sdl2::event::Event::MouseMotion { x, y, .. } => {
                    mouse_pos = (x, y);
                    if let Some(s) = mouse_start_pos {
                        'motion_states: for channel in &mut state {
                            if channel.zoom_rect.contains_point(s) &&
                                    channel.zoom_rect.contains_point(mouse_pos) {
                                let mut nr: Rect = channel.crop.into();
                                let (dx, dy) = scale_point_from_window(mouse_pos, s, WIDTH as i32, HEIGHT as i32, 0, 0);
                                nr.offset(dx, dy);
                                eprintln!("nr = {:?}, w = {}, h = {}", nr, nr.width(), nr.height());
                                let ox = if nr.left() < 0 { -nr.left() } else
                                    if nr.right() >= WIDTH as i32 { WIDTH as i32 - nr.right() - 1 }  else { 0 };
                                let oy = if nr.top() < 0 { -nr.top() } else
                                    if nr.bottom() >= HEIGHT as i32 { HEIGHT as i32 - nr.bottom() - 1 }  else { 0 };
                                nr.offset(ox, oy);
                                if channel.set_crop(nr) {
                                    update_video(& mut canvas, & mut state);
                                }
                                mouse_start_pos = Some(mouse_pos);
                                break 'motion_states;
                            }
                            if channel.full_rect.contains_point(s) &&
                                    channel.full_rect.contains_point(mouse_pos) {
                                let mut nr: Rect = channel.crop.into();
                                let (dx, dy) = scale_point_from_window(s, mouse_pos, WIDTH as i32, HEIGHT as i32, 0, 0);
                                nr.offset(dx, dy);
                                eprintln!("nr = {:?}, w = {}, h = {}", nr, nr.width(), nr.height());
                                let ox = if nr.left() < 0 { -nr.left() } else
                                    if nr.right() >= WIDTH as i32 { WIDTH as i32 - nr.right() - 1 }  else { 0 };
                                let oy = if nr.top() < 0 { -nr.top() } else
                                    if nr.bottom() >= HEIGHT as i32 { HEIGHT as i32 - nr.bottom() - 1 }  else { 0 };
                                nr.offset(ox, oy);
                                if channel.set_crop(nr) {
                                    update_video(& mut canvas, & mut state);
                                }
                                mouse_start_pos = Some(mouse_pos);
                                break 'motion_states;
                            }
                        }
                    }
                },
                sdl2::event::Event::MouseButtonDown { x, y, .. } => {
                    mouse_start_pos = Some((x, y));
                },
                sdl2::event::Event::MouseButtonUp {..} => {
                    mouse_start_pos = None;
                },
                sdl2::event::Event::MouseWheel { y, .. } => {
                    'wheel_states: for channel in &mut state {
                        if channel.zoom_rect.contains_point(mouse_pos) {
                            let r: Rect = channel.crop.into();
                            let p = scale_point_from_window(channel.zoom_rect.top_left().into(), mouse_pos,
                                r.width() as i32, r.height() as i32, r.left(), r.top());
                            let factor = if y > 0 { ZOOM_FACTOR } else { 1.0 / ZOOM_FACTOR };
                            let nw = (r.width()  as f32 * factor) as u32;
                            let nh = (r.height() as f32 * factor) as u32;
                            let nr = if nw >= WIDTH as u32 || nh >= HEIGHT as u32 { FULL_CROP.into() } else {
                                let mut nr = Rect::from_center(p, nw, nh);
                                eprintln!("nr = {:?}, w = {}, h = {}", nr, nw, nh);
                                let ox = if nr.left() < 0 { -nr.left() } else
                                    if nr.right() >= WIDTH as i32 { WIDTH as i32 - nr.right() - 1 }  else { 0 };
                                let oy = if nr.top() < 0 { -nr.top() } else
                                    if nr.bottom() >= HEIGHT as i32 { HEIGHT as i32 - nr.bottom() - 1 }  else { 0 };
                                nr.offset(ox, oy);
                                nr
                            };
                            if channel.set_crop(nr) {
                                update_video(& mut canvas, & mut state);
                            }
                            break 'wheel_states;
                        }
                    }
                },
                _ => {},
            }
        }
        get_video(& mut state);
        update_video(& mut canvas, & mut state);
    }
}

fn scale_point_from_window(point: (i32, i32), offset: (i32, i32), width: i32, height: i32, left: i32, top: i32) -> (i32, i32) {
    let (x, y) = point;
    let (sx, sy) = offset;
    let dx = ((sx - x) *  width) / (WIN_WIDTH  as i32) + left;
    let dy = ((sy - y) * height) / (WIN_HEIGHT as i32) + top;
    (dx, dy)
}

fn sanity_check(state: & mut [ChannelState]) {
    for channel in &mut state.iter_mut() {
        if channel.text_cmd(String::from("get_resolution\n")) != format!("{}x{}\n", WIDTH, HEIGHT) {
            panic!("Invalid resolution, must be {}x{}", WIDTH, HEIGHT);
        }
    }
}

fn get_video(state: & mut [ChannelState]) {
    for channel in &mut state.iter_mut() {
        channel.server.write(b"get_image\n").unwrap();
        channel.server.read_exact(& mut channel.frame).unwrap();
    }
}

fn update_video(canvas: & mut Canvas<Window>, state: & mut [ChannelState]) {
    for channel in &mut state.iter_mut() {
        let surf = Surface::from_data(& mut channel.frame, WIDTH as u32, HEIGHT as u32, 3 * WIDTH as u32, PIXEL_FORMAT).unwrap();

        let tc = canvas.texture_creator();
        let tx = tc.create_texture_from_surface(surf).unwrap();

        canvas.copy(&tx, None, channel.full_rect).unwrap();

        let mut selected: Rect = channel.crop.into();

        let crop = if channel.crop == FULL_CROP { None } else { Some(selected) };

        canvas.copy(&tx, crop, channel.zoom_rect).unwrap();

        selected.set_width (selected.width()  / 2);
        selected.set_height(selected.height() / 2);
        selected.set_x       (selected.x()    / 2);
        selected.set_y       (selected.y()    / 2);
        selected.offset(channel.full_rect.left(), 0);

        canvas.set_draw_color(Color::RGB(255, 0, 0));
        canvas.draw_rect(selected).unwrap();
    }

    // XXX eprintln!("{:?}", canvas.output_size());

    // However the canvas has not been updated to the window yet,
    // everything has been processed to an internal buffer,
    // but if we want our buffer to be displayed on the window,
    // we need to call `present`. We need to call this everytime
    // we want to render a new frame on the window.
    canvas.present();
}
