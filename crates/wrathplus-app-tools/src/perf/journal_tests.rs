//! The FPS journal: the GPU buckets, the per-frame fold, the header and its file layout.
use super::*;
use bevy::diagnostic::{Diagnostic, DiagnosticMeasurement, DiagnosticPath};
use std::time::Duration;

#[test]
fn buckets_name_every_pass_we_draw_and_skip_what_is_not_a_gpu_span() {
    use GpuBucket::*;
    let b = |p: &str| gpu_bucket(p);
    assert_eq!(b("render/main_opaque_pass_3d/elapsed_gpu"), Some(Opaque));
    assert_eq!(b("render/static_gx/elapsed_gpu"), Some(Static));
    assert_eq!(
        b("render/main_transparent_pass_3d/elapsed_gpu"),
        Some(Transparent)
    );
    assert_eq!(b("render/ffx_glow_gauss_h/elapsed_gpu"), Some(Glow));
    assert_eq!(b("render/ffx_glow_combine/elapsed_gpu"), Some(Glow));
    assert_eq!(b("render/tonemapping/elapsed_gpu"), Some(Post));
    assert_eq!(b("render/upscaling/elapsed_gpu"), Some(Post));
    assert_eq!(b("render/ui/elapsed_gpu"), Some(Ui));
    assert_eq!(b("render/ui_gamma_decode/elapsed_gpu"), Some(Ui));
    assert_eq!(b("render/main_transparent_pass_2d/elapsed_gpu"), Some(Ui));
    assert_eq!(
        b("render/early_mesh_preprocessing/elapsed_gpu"),
        Some(Other)
    );
    // CPU spans, non-render diagnostics and nested spans are not GPU cells.
    assert_eq!(b("render/main_opaque_pass_3d/elapsed_cpu"), None);
    assert_eq!(b("fps"), None);
    assert_eq!(b("render/outer/inner/elapsed_gpu"), None);
}

fn push(store: &mut DiagnosticsStore, path: &str, time: Instant, value: f64) {
    let p = DiagnosticPath::new(path.to_string());
    if store.get(&p).is_none() {
        store.add(Diagnostic::new(p.clone()));
    }
    store
        .get_mut(&p)
        .unwrap()
        .add_measurement(DiagnosticMeasurement { time, value });
}

#[test]
fn a_flush_averages_per_frame_read_and_a_fold_takes_only_what_arrived() {
    let mut store = DiagnosticsStore::default();
    let t0 = Instant::now();
    let t1 = t0 + Duration::from_millis(10);
    let t2 = t0 + Duration::from_millis(20);
    // Frame 1: opaque 2, static 3, and the full-screen tail on two cameras (0.5 + 0.3).
    push(
        &mut store,
        "render/main_opaque_pass_3d/elapsed_gpu",
        t0,
        2.0,
    );
    push(&mut store, "render/static_gx/elapsed_gpu", t0, 3.0);
    push(&mut store, "render/tonemapping/elapsed_gpu", t0, 0.5);
    push(&mut store, "render/tonemapping/elapsed_gpu", t0, 0.3);
    // A CPU span beside them, never counted.
    push(
        &mut store,
        "render/main_opaque_pass_3d/elapsed_cpu",
        t0,
        99.0,
    );
    let mut acc = GpuAccum::default();
    acc.fold(&store);
    assert_eq!(acc.frames, 1);
    // Frame 2 lands; the fold reads only it (frame 1's values would double otherwise).
    push(
        &mut store,
        "render/main_opaque_pass_3d/elapsed_gpu",
        t1,
        4.0,
    );
    acc.fold(&store);
    acc.fold(&store); // nothing new: a no-op, not a double count
    assert_eq!(acc.frames, 2);
    let cols = acc.columns();
    // gpu_ms = (2 + 3 + 0.8 + 4) / 2; opaque (2 + 4) / 2; static 3 / 2; post 0.8 / 2.
    assert_eq!(cols, ",4.90,3.00,1.50,0.00,0.00,0.40,0.00,0.00");
    // The reset: a second with nothing read writes empty cells, one per column.
    assert_eq!(acc.columns(), ",,,,,,,,");
    // And a later frame is still read after the reset.
    push(
        &mut store,
        "render/main_opaque_pass_3d/elapsed_gpu",
        t2,
        1.0,
    );
    acc.fold(&store);
    assert_eq!(acc.columns(), ",1.00,1.00,0.00,0.00,0.00,0.00,0.00,0.00");
}

#[test]
fn the_window_columns_sit_after_main_ms_and_before_the_gpu_cells() {
    let cols: Vec<&str> = JOURNAL_HEADER.trim_end().split(',').collect();
    let at = |name: &str| cols.iter().position(|c| *c == name).unwrap();
    assert_eq!(at("occluded_frames"), at("main_ms") + 1);
    assert_eq!(at("unfocused_frames"), at("main_ms") + 2);
    assert_eq!(at("gpu_ms"), at("main_ms") + 3);
}

#[test]
fn a_journal_of_an_older_layout_moves_aside_and_a_current_one_stays() {
    let dir = std::env::temp_dir().join(format!("fps-journal-rotate-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("fps-journal.csv");
    let aside = dir.join("fps-journal.old.csv");
    std::fs::write(&path, format!("# preamble\n{JOURNAL_HEADER}1,2,3\n")).unwrap();
    rotate_if_stale(&path);
    assert!(
        path.exists() && !aside.exists(),
        "the current layout appends in place"
    );
    std::fs::write(&path, "# preamble\nt,x,y,z,mean_ms\n1,2\n").unwrap();
    rotate_if_stale(&path);
    assert!(
        !path.exists() && aside.exists(),
        "an older layout is moved aside"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_header_ends_with_the_gpu_cells_in_bucket_order() {
    let cols: Vec<&str> = JOURNAL_HEADER.trim_end().split(',').collect();
    let gpu: Vec<&str> = cols[cols.len() - (GPU_BUCKETS + 1)..].to_vec();
    assert_eq!(
        gpu,
        [
            "gpu_ms",
            "gpu_opaque",
            "gpu_static",
            "gpu_transp",
            "gpu_glow",
            "gpu_post",
            "gpu_ui",
            "gpu_other"
        ]
    );
    // An empty second writes exactly one cell per GPU column.
    assert_eq!(
        GpuAccum::default().columns().matches(',').count(),
        gpu.len()
    );
}

#[test]
fn the_preamble_names_the_adapter_or_says_it_cannot() {
    assert_eq!(
        preamble(None, None),
        "# wrathplus fps journal | gpu ? | backend ? | driver ? | gpu_spans ?\n"
    );
}
