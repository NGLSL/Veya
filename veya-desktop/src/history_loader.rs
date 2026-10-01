//! Latest-request history loader. Original pixels never enter the capture thread.
use super::{HISTORY_PAGE_SIZE, HISTORY_READ_BATCH};
use crate::capture::{card_view_from_summary, CardView, HistoryQuery};
use crate::format;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
use veya_core::{ClipboardPayload, HistoryAggregator, HistorySummary};
use veya_storage::{Store, Thumbnail};

struct Request {
    generation: u64,
    query: HistoryQuery,
}
#[derive(Default)]
struct Mailbox {
    request: Option<Request>,
    stopped: bool,
}
pub(super) enum ResultEvent {
    Cards {
        generation: u64,
        cards: Vec<CardView>,
        has_next: bool,
    },
    Thumbnail {
        generation: u64,
        sequence: u32,
        hash: String,
        handle: iced::widget::image::Handle,
    },
    ThumbnailUnavailable {
        generation: u64,
        sequence: u32,
        hash: String,
    },
    Error {
        generation: u64,
        message: String,
    },
}
pub(super) struct Loader {
    generation: Arc<AtomicU64>,
    mailbox: Arc<(Mutex<Mailbox>, Condvar)>,
    published: Arc<AtomicU64>,
    pub results: Receiver<ResultEvent>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Loader {
    pub fn spawn(path: PathBuf) -> Self {
        let generation = Arc::new(AtomicU64::new(0));
        let mailbox = Arc::new((Mutex::new(Mailbox::default()), Condvar::new()));
        let (tx, results) = std::sync::mpsc::sync_channel(2);
        let published = Arc::new(AtomicU64::new(0));
        let acknowledged = published.clone();
        let epoch = generation.clone();
        let work = mailbox.clone();
        let thread = std::thread::Builder::new()
            .name("veya-history".into())
            .spawn(move || {
                let mut store = Store::open(&path).map_err(|e| format!("读取历史失败：{e}"));
                loop {
                    let request = {
                        let (lock, ready) = &*work;
                        let mut state = lock.lock().unwrap();
                        while state.request.is_none() && !state.stopped {
                            state = ready.wait(state).unwrap();
                        }
                        if state.stopped {
                            break;
                        }
                        state.request.take().unwrap()
                    };
                    let current = || epoch.load(Ordering::Acquire) == request.generation;
                    let result = match &mut store {
                        Ok(store) => load_page(store, &request.query, &current),
                        Err(message) => Err(message.clone()),
                    };
                    match result {
                        Ok(Some((cards, has_next))) => {
                            let images: Vec<_> = cards
                                .iter()
                                .filter_map(|card| match card.payload {
                                    crate::capture::CardPayloadView::Image { .. } => {
                                        Some((card.sequence, card.content_hash.clone()))
                                    }
                                    _ => None,
                                })
                                .collect();
                            if !send(
                                &tx,
                                ResultEvent::Cards {
                                    generation: request.generation,
                                    cards,
                                    has_next,
                                },
                                &current,
                            ) {
                                continue;
                            }
                            while current()
                                && acknowledged.load(Ordering::Acquire) != request.generation
                            {
                                std::thread::sleep(Duration::from_millis(5));
                            }
                            if !current() {
                                continue;
                            }
                            if let Ok(store) = &mut store {
                                for (sequence, hash) in images {
                                    if !current() {
                                        break;
                                    }
                                    if let Some(thumbnail) =
                                        load_thumbnail(store, sequence, &hash, &current)
                                    {
                                        let handle = iced::widget::image::Handle::from_rgba(
                                            thumbnail.width,
                                            thumbnail.height,
                                            thumbnail.rgba,
                                        );
                                        if !send(
                                            &tx,
                                            ResultEvent::Thumbnail {
                                                generation: request.generation,
                                                sequence,
                                                hash,
                                                handle,
                                            },
                                            &current,
                                        ) {
                                            break;
                                        }
                                    } else if current()
                                        && !send(
                                            &tx,
                                            ResultEvent::ThumbnailUnavailable {
                                                generation: request.generation,
                                                sequence,
                                                hash,
                                            },
                                            &current,
                                        )
                                    {
                                        break;
                                    }
                                }
                            }
                        }
                        Ok(None) => {}
                        Err(message) => {
                            send(
                                &tx,
                                ResultEvent::Error {
                                    generation: request.generation,
                                    message,
                                },
                                &current,
                            );
                        }
                    }
                }
            })
            .expect("spawn history loader");
        Self {
            generation,
            mailbox,
            published,
            results,
            thread: Some(thread),
        }
    }
    pub fn acknowledge(&self, generation: u64) {
        self.published.store(generation, Ordering::Release);
    }
    pub fn cancel(&self) -> u64 {
        self.generation
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1)
    }
    pub fn request(&self, query: &HistoryQuery) -> u64 {
        let generation = self.cancel();
        let (lock, ready) = &*self.mailbox;
        lock.lock().unwrap().request = Some(Request {
            generation,
            query: query.clone(),
        });
        ready.notify_one();
        generation
    }
}
impl Drop for Loader {
    fn drop(&mut self) {
        self.cancel();
        let (lock, ready) = &*self.mailbox;
        let mut state = lock.lock().unwrap();
        state.stopped = true;
        state.request = None;
        ready.notify_one();
        drop(state);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
fn send(tx: &SyncSender<ResultEvent>, mut event: ResultEvent, current: &impl Fn() -> bool) -> bool {
    while current() {
        match tx.try_send(event) {
            Ok(()) => return true,
            Err(TrySendError::Full(returned)) => {
                event = returned;
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(TrySendError::Disconnected(_)) => return false,
        }
    }
    false
}
fn load_thumbnail(
    store: &mut Store,
    sequence: u32,
    hash: &str,
    current: &impl Fn() -> bool,
) -> Option<Thumbnail> {
    if let Some(cached) = store.load_thumbnail(sequence, hash).ok().flatten() {
        return Some(cached);
    }
    if !current() {
        return None;
    }
    let record = store.load_record(sequence).ok().flatten()?;
    if record.content_hash != hash {
        return None;
    }
    let ClipboardPayload::Image { png, .. } = record.payload else {
        return None;
    };
    if !current() {
        return None;
    }
    let thumbnail = decode_thumbnail(&png, current)?;
    if !current() {
        return None;
    }
    let _ = store.save_thumbnail(sequence, hash, &thumbnail);
    Some(thumbnail)
}

/// The captured 8-bit, noninterlaced PNG path only retains decoder rows and
/// thumbnail accumulators. Box averages match image::thumbnail's downsampling.
/// Small, 16-bit and interlaced inputs retain the existing decoder behavior.
fn decode_thumbnail(bytes: &[u8], current: &impl Fn() -> bool) -> Option<Thumbnail> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_ignore_text_chunk(true);
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().ok()?;
    let info = reader.info();
    let (width, height) = (info.width, info.height);
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 16 * 1024 * 1024 {
        return None;
    }
    if info.interlaced || info.bit_depth == png::BitDepth::Sixteen || width.max(height) <= 128 {
        drop(reader);
        if !current() {
            return None;
        }
        let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Png).ok()?;
        if !current() {
            return None;
        }
        let small = image.thumbnail(128, 128).into_rgba8();
        return Some(Thumbnail {
            width: small.width(),
            height: small.height(),
            rgba: small.into_raw(),
        });
    }
    let scale = 128.0 / f64::from(width.max(height));
    let out_width = (f64::from(width) * scale).round().max(1.0) as u32;
    let out_height = (f64::from(height) * scale).round().max(1.0) as u32;
    let (color, depth) = reader.output_color_type();
    if depth != png::BitDepth::Eight {
        return None;
    }
    let channels = match color {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return None,
    };
    let bounds = |size: u32, out: u32| -> Vec<(u32, u32)> {
        let ratio = size as f32 / out as f32;
        (0..out)
            .map(|n| {
                let low = n as f32 * ratio;
                let start = (low.ceil() as u32).min(size - 1);
                let end = ((low + ratio).ceil() as u32).clamp(start + 1, size);
                (start, end)
            })
            .collect()
    };
    let columns = bounds(width, out_width);
    let rows = bounds(height, out_height);
    let mut sums = vec![[0u64; 4]; (out_width * out_height) as usize];
    for y in 0..height {
        if !current() {
            return None;
        }
        let row = reader.next_row().ok()??;
        let data = row.data();
        if data.len() != width as usize * channels {
            return None;
        }
        for (out_y, &(bottom, top)) in rows.iter().enumerate() {
            if !(bottom..top).contains(&y) {
                continue;
            }
            for (out_x, &(left, right)) in columns.iter().enumerate() {
                let sum = &mut sums[out_y * out_width as usize + out_x];
                for x in left..right {
                    let p = &data[x as usize * channels..][..channels];
                    let pixel = match color {
                        png::ColorType::Grayscale => [p[0], p[0], p[0], 255],
                        png::ColorType::GrayscaleAlpha => [p[0], p[0], p[0], p[1]],
                        png::ColorType::Rgb => [p[0], p[1], p[2], 255],
                        png::ColorType::Rgba => [p[0], p[1], p[2], p[3]],
                        png::ColorType::Indexed => unreachable!(),
                    };
                    for (channel, value) in sum.iter_mut().zip(pixel) {
                        *channel += u64::from(value);
                    }
                }
            }
        }
    }
    if !current() {
        return None;
    }
    reader.finish().ok()?;
    let mut rgba = Vec::with_capacity((out_width * out_height * 4) as usize);
    for (out_y, &(bottom, top)) in rows.iter().enumerate() {
        for (out_x, &(left, right)) in columns.iter().enumerate() {
            let count = u64::from(right - left) * u64::from(top - bottom);
            for channel in sums[out_y * out_width as usize + out_x] {
                rgba.push(((channel + count / 2) / count) as u8);
            }
        }
    }
    Some(Thumbnail {
        width: out_width,
        height: out_height,
        rgba,
    })
}
fn matches(summary: &HistorySummary, query: &HistoryQuery) -> bool {
    let record = &summary.representative;
    let kind = format::history_kind(&record.payload, &record.content);
    query.filter.matches(kind)
        && (!query.pinned_only || record.pinned)
        && veya_core::match_field(
            &record.content,
            &record.source_app,
            summary.used_in.iter().map(|u| u.target_app.as_str()),
            &query.search,
        )
        .is_some()
}
pub(super) fn load_page(
    store: &Store,
    query: &HistoryQuery,
    current: &impl Fn() -> bool,
) -> Result<Option<(Vec<CardView>, bool)>, String> {
    let mut cursor = None;
    let mut aggregator = HistoryAggregator::new(query.newest_first);
    let mut cards = Vec::with_capacity(HISTORY_PAGE_SIZE);
    let mut matched = 0usize;
    let skip = query.page.saturating_mul(HISTORY_PAGE_SIZE);
    let mut consume = |summary: HistorySummary| {
        if matches(&summary, query) {
            if matched >= skip.saturating_add(HISTORY_PAGE_SIZE) {
                return true;
            }
            if matched >= skip {
                cards.push(card_view_from_summary(&summary, None));
            }
            matched = matched.saturating_add(1);
        }
        false
    };
    loop {
        if !current() {
            return Ok(None);
        }
        let batch = store
            .load_history_page(cursor, HISTORY_READ_BATCH, query.newest_first)
            .map_err(|e| format!("读取历史失败：{e}"))?;
        let count = batch.len();
        for record in batch {
            if !current() {
                return Ok(None);
            }
            cursor = Some((record.created_at_ms, record.sequence));
            if let Some(summary) = aggregator.push(record) {
                if consume(summary) {
                    return Ok(Some((cards, true)));
                }
            }
        }
        if count < HISTORY_READ_BATCH {
            break;
        }
    }
    let has_next = aggregator.finish().is_some_and(consume);
    Ok(Some((cards, has_next)))
}

#[cfg(test)]
mod tests {
    use super::super::tests::text_record;
    use super::*;
    use std::sync::atomic::AtomicUsize;
    static NEXT: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn row_thumbnail_preserves_existing_pixels_dimensions_and_alpha() {
        for (width, height) in [(2048, 2048), (401, 203), (1, 511), (127, 9), (2, 2)] {
            let original = image::DynamicImage::ImageRgba8(image::RgbaImage::from_fn(
                width,
                height,
                |x, y| {
                    image::Rgba([
                        (x % 256) as u8,
                        (y % 256) as u8,
                        ((x + y) % 256) as u8,
                        ((x * 3 + y * 7) % 256) as u8,
                    ])
                },
            ));
            let mut png = Vec::new();
            original
                .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                .unwrap();
            let expected = original.thumbnail(128, 128).into_rgba8();
            let actual = decode_thumbnail(&png, &|| true).unwrap();
            assert_eq!((actual.width, actual.height), expected.dimensions());
            assert_eq!(actual.rgba, expected.into_raw(), "{width}x{height}");
        }
    }

    #[test]
    fn row_thumbnail_preserves_rgb_gray_palette_and_16_bit_inputs() {
        let inputs = [
            image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(256, 129, |x, y| {
                image::Rgb([x as u8, y as u8, 37])
            })),
            image::DynamicImage::ImageLuma8(image::GrayImage::from_fn(256, 129, |x, y| {
                image::Luma([(x + y) as u8])
            })),
            image::DynamicImage::ImageLumaA8(image::GrayAlphaImage::from_fn(256, 129, |x, y| {
                image::LumaA([x as u8, y as u8])
            })),
            image::DynamicImage::ImageLuma16(image::ImageBuffer::from_fn(256, 129, |x, y| {
                image::Luma([(x * 256 + y) as u16])
            })),
        ];
        for original in inputs {
            let mut png = Vec::new();
            original
                .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                .unwrap();
            assert_eq!(
                decode_thumbnail(&png, &|| true).unwrap().rgba,
                original.thumbnail(128, 128).into_rgba8().into_raw()
            );
        }
        let mut indexed = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut indexed, 256, 129);
            encoder.set_color(png::ColorType::Indexed);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_palette(vec![10, 20, 30, 90, 100, 110]);
            encoder.set_trns(vec![0, 255]);
            let mut writer = encoder.write_header().unwrap();
            writer
                .write_image_data(&(0..256 * 129).map(|n| (n % 2) as u8).collect::<Vec<_>>())
                .unwrap();
        }
        let expected = image::load_from_memory(&indexed)
            .unwrap()
            .thumbnail(128, 128)
            .into_rgba8();
        assert_eq!(
            decode_thumbnail(&indexed, &|| true).unwrap().rgba,
            expected.into_raw()
        );
    }

    #[test]
    fn row_decode_honors_cancellation_and_rejects_truncated_images() {
        assert!(decode_thumbnail(b"invalid PNG", &|| true).is_none());
        let original = image::DynamicImage::ImageRgba8(image::RgbaImage::new(256, 256));
        let mut png = Vec::new();
        original
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let calls = AtomicUsize::new(0);
        assert!(decode_thumbnail(&png, &|| calls.fetch_add(1, Ordering::Relaxed) < 8).is_none());
        png.truncate(png.len() / 2);
        assert!(decode_thumbnail(&png, &|| true).is_none());
    }
    struct Database(PathBuf);
    impl Database {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!(
                "veya-lazy-{}-{}.db",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            )))
        }
    }
    impl Drop for Database {
        fn drop(&mut self) {
            for suffix in ["", "-wal", "-shm"] {
                let _ = std::fs::remove_file(format!("{}{}", self.0.display(), suffix));
            }
        }
    }
    #[test]
    fn invalid_png_finishes_as_unavailable_without_hiding_the_record() {
        let db = Database::new();
        let mut store = Store::open(&db.0).unwrap();
        let mut record = text_record(1, "", 10_000);
        record.payload = ClipboardPayload::Image {
            png: b"invalid PNG".to_vec(),
            width: 10,
            height: 10,
        };
        store.insert_record(&record).unwrap();
        let loader = Loader::spawn(db.0.clone());
        let generation = loader.request(&HistoryQuery::default());
        let ResultEvent::Cards { cards, .. } =
            loader.results.recv_timeout(Duration::from_secs(2)).unwrap()
        else {
            panic!("cards must arrive first")
        };
        assert_eq!(cards[0].sequence, 1);
        assert!(matches!(
            cards[0].payload,
            crate::capture::CardPayloadView::Image {
                thumbnail_failed: false,
                ..
            }
        ));
        loader.acknowledge(generation);
        assert!(matches!(
            loader.results.recv_timeout(Duration::from_secs(2)).unwrap(),
            ResultEvent::ThumbnailUnavailable { sequence: 1, .. }
        ));
        assert!(store
            .load_thumbnail(1, &record.content_hash)
            .unwrap()
            .is_none());
    }
    #[test]
    fn image_cards_are_published_before_decode_and_cached_for_reopen() {
        let db = Database::new();
        let mut store = Store::open(&db.0).unwrap();
        let mut record = text_record(1, "", 10_000);
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            256,
            256,
            image::Rgba([1, 2, 3, 255]),
        ))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
        record.payload = ClipboardPayload::Image {
            png,
            width: 256,
            height: 256,
        };
        record.content_hash = veya_core::payload_hash(&record.payload);
        store.insert_record(&record).unwrap();
        let loader = Loader::spawn(db.0.clone());
        let generation = loader.request(&HistoryQuery::default());
        match loader.results.recv_timeout(Duration::from_secs(5)).unwrap() {
            ResultEvent::Cards {
                generation: actual,
                cards,
                ..
            } => {
                assert_eq!(actual, generation);
                assert!(matches!(
                    cards[0].payload,
                    crate::capture::CardPayloadView::Image { handle: None, .. }
                ));
            }
            _ => panic!("cards must be first"),
        }
        assert!(loader
            .results
            .recv_timeout(Duration::from_millis(50))
            .is_err());
        assert!(store
            .load_thumbnail(1, &record.content_hash)
            .unwrap()
            .is_none());
        loader.acknowledge(generation);
        match loader.results.recv_timeout(Duration::from_secs(5)).unwrap() {
            ResultEvent::Thumbnail {
                generation: actual,
                sequence,
                ..
            } => {
                assert_eq!(actual, generation);
                assert_eq!(sequence, 1);
            }
            _ => panic!("thumbnail expected after publication"),
        }
        let thumbnail = store
            .load_thumbnail(1, &record.content_hash)
            .unwrap()
            .unwrap();
        assert_eq!(
            (thumbnail.width, thumbnail.height, thumbnail.rgba.len()),
            (128, 128, 128 * 128 * 4)
        );
        drop(loader);
        let loader = Loader::spawn(db.0.clone());
        let generation = loader.request(&HistoryQuery::default());
        assert!(matches!(
            loader.results.recv_timeout(Duration::from_secs(5)).unwrap(),
            ResultEvent::Cards { .. }
        ));
        loader.acknowledge(generation);
        assert!(matches!(
            loader.results.recv_timeout(Duration::from_secs(5)).unwrap(),
            ResultEvent::Thumbnail { .. }
        ));
    }
    #[test]
    fn latest_request_supersedes_unacknowledged_page_and_searches_full_text_tail() {
        let db = Database::new();
        let mut store = Store::open(&db.0).unwrap();
        for sequence in 1..=30 {
            store
                .insert_record(&text_record(
                    sequence,
                    &format!("entry-{sequence}"),
                    i64::from(sequence) * 10_000,
                ))
                .unwrap();
        }
        let full = format!("{}TAIL_UNIQUE", "x".repeat(10_000));
        store
            .insert_record(&text_record(31, &full, 310_000))
            .unwrap();
        let loader = Loader::spawn(db.0.clone());
        let original = loader.request(&HistoryQuery::default());
        assert!(
            matches!(loader.results.recv_timeout(Duration::from_secs(5)).unwrap(),ResultEvent::Cards { generation,.. } if generation==original)
        );
        let query = HistoryQuery {
            search: "TAIL_UNIQUE".into(),
            ..HistoryQuery::default()
        };
        let latest = loader.request(&query);
        match loader.results.recv_timeout(Duration::from_secs(5)).unwrap() {
            ResultEvent::Cards {
                generation,
                cards,
                has_next,
            } => {
                assert_eq!(generation, latest);
                assert_eq!(cards.len(), 1);
                assert_eq!(cards[0].sequence, 31);
                assert!(cards[0].content_excerpt.chars().count() <= 512);
                assert!(!cards[0].content_excerpt.contains("TAIL_UNIQUE"));
                assert!(!has_next);
            }
            _ => panic!("latest searchable page expected"),
        }
        loader.cancel();
        assert!(loader
            .results
            .recv_timeout(Duration::from_millis(50))
            .is_err());
    }
}
