// SPDX-License-Identifier: MIT
// Times a document at a chosen size on one thread and on every core and compares the levels.
// With STREAM=1 the render is the add-on's: scattered, with every pixel handed to an observer.
// usage: bench_threads DOC VERSION(3.0|3.1|5.0) WIDTH HEIGHT [THREADS] with the substrate in SUBSTRATE (default: none found).
use std::time::Instant;
use stecyk3d::cases::picture;
use stecyk3d::render::{OpenChoices, render_lit, render_lit_streamed};
use stecyk3d::scene::Scene;
use stecyk3d::scene_file::Flavour;
use stecyk3d::version::RenderAs;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut scene = Scene::read(&std::fs::read(&a[1]).unwrap(), Flavour::Bryce3d).unwrap();
    let v = match a[2].as_str() { "3.0" => RenderAs::Bryce30, "3.1" => RenderAs::Bryce31, _ => RenderAs::Bryce50 };
    scene.settings.picture_width = a[3].parse().unwrap();
    scene.settings.picture_height = a[4].parse().unwrap();
    scene.settings.region.top = 0;
    scene.settings.region.left = 0;
    scene.settings.region.bottom = scene.settings.picture_height;
    scene.settings.region.right = scene.settings.picture_width;
    let threads: usize = a.get(5).map_or(0, |t| t.parse().unwrap());
    let sub = stecyk3d::substrate::find_and_load(std::env::var_os("SUBSTRATE").map(std::path::PathBuf::from).as_deref(), None).unwrap();
    let run = |t: usize| {
        let stream = std::env::var_os("STREAM").is_some();
        let c = OpenChoices { threads: Some(t), scattered: stream.then_some(1), ..picture::choices_from(&sub) };
        let t0 = Instant::now();
        let mut got = 0u64;
        let r = if stream { render_lit_streamed(&scene, v, &c, &mut |_, _, l| got += l.len() as u64) } else { render_lit(&scene, v, &c) }.map(|(p, _)| p.levels).map_err(|e| e.to_string());
        (t0.elapsed().as_secs_f64(), r)
    };
    let (ts, s) = run(1);
    let (tp, p) = run(threads);
    println!("serial {ts:.2}s  threads({threads}) {tp:.2}s  speedup {:.2}x  identical {} ({})", ts / tp, s == p, match &s { Ok(_) => "ok".to_string(), Err(e) => e.chars().take(80).collect() });
}
