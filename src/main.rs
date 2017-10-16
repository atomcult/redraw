///////////////////////////////////////////////////////////////////////////
// ReDraw                                                                //
// Copyright © 2017 zxv                                                  //
//                                                                       //
// This program is free software: you can redistribute it and/or modify  //
// it under the terms of the GNU General Public License as published by  //
// the Free Software Foundation, either version 3 of the License, or     //
// (at your option) any later version.                                   //
//                                                                       //
// This program is distributed in the hope that it will be useful,       //
// but WITHOUT ANY WARRANTY; without even the implied warranty of        //
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the         //
// GNU General Public License for more details.                          //
//                                                                       //
// You should have received a copy of the GNU General Public License     //
// along with this program.  If not, see <http://www.gnu.org/licenses/>. //
///////////////////////////////////////////////////////////////////////////

#![feature(plugin)]
#![plugin(docopt_macros)]

extern crate rand;
extern crate image;

#[macro_use]
extern crate serde_derive;
extern crate docopt;

use std::path::Path;
use std::io::{stdout,Write};
use rand::distributions::{IndependentSample,Range};
use image::RgbImage;

include!(concat!(env!("OUT_DIR"), "/version.rs"));

docopt!(Args derive Debug, "
ReDraw

Usage:
  redraw [options] FILE
  redraw -h | --help
  redraw --version

Options:
  -h, --help                Show this help.
  --version                 Print version information.
  -q, --quiet               Suppress output.
  -o, --output FILE         Output to FILE. [default: redraw.png]
  -n, --iterate N           Set number of iterations. [default: 500000]
  -m, --min MIN             Set minimum size of each drawn object. [default: 1]
  -M, --max MAX             Set maximum size of each drawn object. [default: 20]
  --shapes SHAPES           List of shapes for generating delimited by a comma. [default: lines]
  --uniform                 Sample a uniform color distribution.
  -a, --adaptive            Reduce object size after certain number of failures.
  --adapt-rate C            Number of failures before reducing size. [default: 100000]
  --adapt-coeff K           Amount to reduce object size by. [default: 0.9]
  -A, --animate             Output frames for animation.
  --animation-interval N    Output image every N objects drawn. [default: 1000]
  -b, --blur                Apply gaussian blur.
  --blur-amount AMOUNT      Set blur level. [default: 0.5]
  -B, --bias                Make line angle have bias.
  -D, --DEBUG               DEBUG

Shapes:
  lines
  rectangles
", flag_iterate:            u64,
   flag_min:                u32,
   flag_max:                u32,
   flag_adapt_rate:         u64,
   flag_adapt_coeff:        f64,
   flag_animation_interval: u64,
   flag_blur_amount:        f32);

fn main() {
    // Parse arguments and set defaults
    let args: Args = Args::docopt().deserialize()
        .unwrap_or_else(|e| e.exit());

    if args.flag_version { println!("{}", version()); return }

    // FIXME: DEBUG
    if args.flag_DEBUG { println!("{:?}", args); }

    // Parse and initialize shapes vec
    let shapes = args.flag_shapes.split(',');
    let mut shape_fns: Vec<&Fn(u32, u32, u32, u32) -> Vec<(u32, u32)>> = Vec::new();
    for shape in shapes {
        match shape {
            "lines" => shape_fns.push(&gen_line),
            "rectangles" => shape_fns.push(&gen_rect),
            o => {
                println!("Error: `{}` is not a valid shape!", o);
                return
            }
        }
    }
    let shape_fns = shape_fns;

    // Load image
    let img = image::open(&Path::new(&args.arg_FILE)).unwrap();
    let img = img.to_rgb();
    let (x_max, y_max) = img.dimensions();

    // Generate palette
    // TODO: This may not have to be created by using the same indexing
    //       within the main loop
    let mut palette: Vec<[u8;3]> = Vec::new();
    {
        let img_raw = img.clone().into_raw();
        for i in 0..img.len()/3 {
            palette.push(owned_array(&img_raw[3*i..3*(i+1)]))
        }
    }
    if args.flag_uniform {
        palette.sort();
        palette.dedup();
    }

    // Initialize canvas
    let mut canv = RgbImage::new(x_max,y_max);

    // Loop settings
    // FIXME: iter doesn't need to be set, flag_iterate can be access directly
    let iter = args.flag_iterate;
    let mut max = args.flag_max;
    let mut num_objs = 0;
    let mut adapt_counter = 0;

    // Initialize RNG
    let mut rng = rand::thread_rng();
    let x_rng = Range::new(0, x_max);
    let y_rng = Range::new(0, y_max);
    let mut offset_rng = Range::new((args.flag_min as f64)/(max as f64), 1.);
    let color_rng = Range::new(0, palette.len());
    let fn_rng = Range::new(0, shape_fns.len());

    // Begin main loop
    let prog = args.flag_iterate / 100;
    for i in 0..iter {
        // Generate random line and color
        let x0 = x_rng.ind_sample(&mut rng);
        let y0 = y_rng.ind_sample(&mut rng);
        let (x1, y1);
        let color = palette[color_rng.ind_sample(&mut rng)];

        // FIXME! Overhaul needed!!!
        let y = y0 as f64 + max as f64 * offset_rng.ind_sample(&mut rng);
        if !args.flag_bias {
            // Make there isn't integer rollover from negative values
            let x = x0 as f64 + if i % 2 == 0 { -1. } else { 1. } * max as f64 * offset_rng.ind_sample(&mut rng);
            if x < 0. {
                let y_int = y0 as f64 - x0 as f64*(y - y0 as f64)/(x - x0 as f64);
                if y_int >= 0. {
                    x1 = 0;
                    y1 = y_int as u32;
                } else {
                    x1 = (x0 as f64 - y0 as f64*(x - x0 as f64)/(y - y0 as f64)) as u32;
                    y1 = 0;
                }
            } else {
                x1 = x as u32;
                y1 = y as u32;
            }
        } else {
            y1 = y as u32;
            x1 = (x0 as f64 + max as f64 * offset_rng.ind_sample(&mut rng)) as u32;
        }

        let object = shape_fns[fn_rng.ind_sample(&mut rng)](x0, y0, x1, y1);

        // Calculate RGB distance
        let mut d_buf = 0;
        let mut d_canv = 0;
        for &(x,y) in &object {
            if x >= x_max || y >= y_max { continue };
            let pt_img  = img.get_pixel(x,y).data;
            let pt_canv = canv.get_pixel(x,y).data;

            d_buf  += ((pt_img[0] as i64 - pt_canv[0] as i64).abs()
                    +  (pt_img[1] as i64 - pt_canv[1] as i64).abs()
                    +  (pt_img[2] as i64 - pt_canv[2] as i64).abs()) as u32;

            d_canv += ((pt_img[0] as i64 - color[0] as i64).abs()
                    +  (pt_img[1] as i64 - color[1] as i64).abs()
                    +  (pt_img[2] as i64 - color[2] as i64).abs()) as u32;
        }

        // FIXME
        if !args.flag_quiet && i % prog == 0 {
            print!("\r{}% ({})", i / prog, num_objs);
            stdout().flush().unwrap();
        }

        if args.flag_adaptive && (i - num_objs) > (2u64.pow(adapt_counter) * args.flag_adapt_rate) {
            max = (max as f64 * args.flag_adapt_coeff) as u32;
            if max <= args.flag_min { max = args.flag_min + 1; }
            if args.flag_DEBUG { println!("\tMAX: {}", max); }
            offset_rng = Range::new((args.flag_min as f64)/(max as f64), 1.);
            adapt_counter += 1;
        }

        // Draw obj if it's an improvement
        if d_canv < d_buf {
            draw(&mut canv, &object, color);
            num_objs += 1;
        }

        // Save animation frames
        if args.flag_animate && num_objs % args.flag_animation_interval == 0 {
            canv.save(format!("frame-{:05}.jpg", num_objs / args.flag_animation_interval)).unwrap();
        }
    }

    let file = Path::new(&args.flag_output);
    if args.flag_blur {
        image::imageops::blur(&canv, args.flag_blur_amount);
    }
    canv.save(&file).unwrap();
    if !args.flag_quiet { println!("\r100% ({})", num_objs); }
}

fn owned_array<T: Copy>(slice: &[T]) -> [T; 3] {
    [slice[0], slice[1], slice[2]]
}

fn draw(img: &mut image::RgbImage, points: &[(u32,u32)], color: [u8;3]) {

    let (x_max, y_max) = img.dimensions();

    for pt in points {
        let (x,y) = *pt;
        if x < x_max && y < y_max {
            // Set pixel
            img.get_pixel_mut(x as u32, y as u32).data = color;
        };
    }
}

// TODO: Add width parameter
fn gen_line(x0: u32, y0: u32, x1: u32, y1: u32) -> Vec<(u32, u32)> {
    let mut line = Vec::new();

    // Create local variables for moving start point
    let mut x0 = x0 as i64;
    let mut y0 = y0 as i64;
    let x1 = x1 as i64;
    let y1 = y1 as i64;

    // Get absolute x/y offset
    let dx = (x0 - x1).abs();
    let dy = (y0 - y1).abs();

    // Get slopes
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };

    // Initialize error
    let mut err = if dx > dy { dx } else { -dy } / 2;
    let mut err2;

    line.push((x0 as u32, y0 as u32));
    loop {
        // Check end condition
        if x0 == x1 && y0 == y1 { break };

        // Store old error
        err2 = 2 * err;

        // Adjust error and start position
        if err2 > -dx { err -= dy; x0 += sx; }
        if err2 < dy { err += dx; y0 += sy; }

        line.push((x0 as u32, y0 as u32));
    }
    line.push((x1 as u32, y1 as u32));

    line
}

fn gen_rect(x0: u32, y0: u32, x1: u32, y1: u32) -> Vec<(u32, u32)> {
    let mut rect = Vec::new();

    for x in x0..x1 {
        for y in y0..y1 {
            rect.push((x, y));
        }
    }
    rect
}

fn version() -> String {
    format!("ReDraw {} ({}), compiled on {}.\nCheck your copy-privilege. 🄯  2017",
        semver(), target(), short_now())
}
