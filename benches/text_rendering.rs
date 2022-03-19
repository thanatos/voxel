use criterion::{black_box, criterion_group, criterion_main, Criterion};

use voxel;

pub fn bench_render_text_with_cached_glyphs(c: &mut Criterion) {
    let mut fonts = voxel::resources::Fonts::init(true).unwrap();
    let empty_cache = voxel::text_rendering::cache::GlyphCache::empty(14 << 6);
    c.bench_function("render text, no cache", |b| {
        b.iter(|| {
            let color = voxel::sw_image::Pixel {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            };
            let face = &mut fonts.deja_vu;
            voxel::text_rendering::render_text(
                black_box("Hello, world."),
                black_box(face),
                black_box(color),
                &empty_cache,
            )
            .unwrap();
        })
    });

    c.bench_function("render text, with cache", |b| {
        b.iter(|| {
            let color = voxel::sw_image::Pixel {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            };
            let face = &mut fonts.deja_vu;
            voxel::text_rendering::render_text(
                black_box("Hello, world."),
                black_box(face),
                black_box(color),
                &fonts.deja_vu_cache,
            )
            .unwrap();
        })
    });
}

criterion_group!(benches, bench_render_text_with_cached_glyphs);
criterion_main!(benches);
