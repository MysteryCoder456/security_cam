use std::{
    io::Cursor,
    sync::Arc,
    time::{Duration, Instant},
};

use drm_fourcc::DrmFourcc;
use image::{ImageBuffer, ImageFormat, RgbImage};
use libcamera::{
    camera::CameraConfigurationStatus,
    camera_manager::CameraManager,
    framebuffer_allocator::{FrameBuffer, FrameBufferAllocator},
    framebuffer_map::MemoryMappedFrameBuffer,
    geometry::Size,
    pixel_format::PixelFormat,
    request::{Request, ReuseFlag},
    stream::{Stream, StreamRole},
};
use tokio::sync::broadcast;

pub type FrameData = Vec<u8>;

const MAX_FRAME_RATE: f32 = 30.;
const PIXEL_FORMAT: PixelFormat = PixelFormat::new(DrmFourcc::Bgr888 as u32, 0);
const FRAME_SIZE: Size = Size {
    width: 1024,
    height: 768,
};

fn extract_image_from_request(request: &mut Request, camera_stream: &Stream) -> RgbImage {
    let framebuffer: &MemoryMappedFrameBuffer<FrameBuffer> = request.buffer(camera_stream).unwrap();
    let planes = framebuffer.data();
    let frame_data = planes.get(0).unwrap();
    ImageBuffer::from_raw(FRAME_SIZE.width, FRAME_SIZE.height, frame_data.to_vec()).unwrap()
}

pub fn footage_capture_task(footage_tx: Arc<broadcast::Sender<FrameData>>) {
    // Initialize camera
    let cam_mgr = CameraManager::new().unwrap();
    let cameras = cam_mgr.cameras();
    let cam = cameras.get(0).unwrap();
    let mut cam = cam.acquire().unwrap();

    // Configure camera
    let mut cfgs = cam
        .generate_configuration(&[StreamRole::VideoRecording])
        .unwrap();
    cfgs.get_mut(0).unwrap().set_pixel_format(PIXEL_FORMAT);
    cfgs.get_mut(0).unwrap().set_size(FRAME_SIZE);

    match cfgs.validate() {
        CameraConfigurationStatus::Valid => println!("Camera configuration valid!"),
        CameraConfigurationStatus::Adjusted => {
            println!("Camera configuration was adjusted: {:#?}", cfgs)
        }
        CameraConfigurationStatus::Invalid => panic!("Error validating camera configuration"),
    }

    cam.configure(&mut cfgs).unwrap();

    // Allocate framebuffers
    let mut alloc = FrameBufferAllocator::new(&cam);
    let cfg = cfgs.get(0).unwrap();
    let stream = cfg.stream().unwrap();
    let buffers = alloc.alloc(&stream).unwrap();

    let buffers = buffers
        .into_iter()
        .map(|buf| MemoryMappedFrameBuffer::new(buf).unwrap())
        .collect::<Vec<_>>();

    // Create requests and bind the allocated buffers
    let reqs = buffers
        .into_iter()
        .enumerate()
        .map(|(i, buf)| {
            let mut req = cam.create_request(Some(i as u64)).unwrap();
            req.add_buffer(&stream, buf).unwrap();
            req
        })
        .collect::<Vec<_>>();

    // Create request completion callback
    let (req_tx, req_rx) = std::sync::mpsc::channel();
    cam.on_request_completed(move |req| {
        req_tx.send(req).unwrap();
    });

    // Start camera and queue capture requests
    cam.start(None).unwrap();
    for req in reqs {
        cam.queue_request(req).unwrap();
    }

    let mut img_bytes = Vec::new();
    let mut start = Instant::now();
    let mut end = Instant::now();

    loop {
        std::thread::sleep(Duration::from_millis(
            (1000. / MAX_FRAME_RATE - (end - start).as_millis() as f32) as u64,
        ));

        if footage_tx.receiver_count() < 1 {
            std::thread::sleep(Duration::from_secs(1));
            continue;
        }

        start = Instant::now();

        // Receive capture request and extract image from it
        let mut req = req_rx.recv().unwrap();
        let img = extract_image_from_request(&mut req, &stream);

        // Write image bytes to a vector and broadcast
        img.write_to(&mut Cursor::new(&mut img_bytes), ImageFormat::Jpeg)
            .unwrap();
        _ = footage_tx.send(img_bytes.clone());

        // Reuse request
        req.reuse(ReuseFlag::REUSE_BUFFERS);
        cam.queue_request(req).unwrap();

        end = Instant::now();
    }
}
