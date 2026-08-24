use glgpu_core::{Extent2d, PixelCoord, Rgba8, ScreenVertex, Triangle};
use glgpu_shader::ShaderStage;
use glgpu_trace::PixelEventOutcome;
use glgpu_webgpu::{
    Capability, CommandEncodingError, CpuGpuSubset, DeviceDescriptor, SupportStatus,
};

fn device(width: u32, height: u32) -> glgpu_webgpu::CpuDevice {
    CpuGpuSubset::new()
        .request_adapter()
        .request_device(DeviceDescriptor::new(Extent2d::new(width, height)))
        .expect("test device")
}

fn red_triangle() -> Triangle {
    Triangle::from_vertices(
        7,
        ScreenVertex::new(1.0, 1.0, 0.5, Rgba8::new(255, 0, 0, 255)),
        ScreenVertex::new(7.0, 1.0, 0.5, Rgba8::new(255, 0, 0, 255)),
        ScreenVertex::new(1.0, 7.0, 0.5, Rgba8::new(255, 0, 0, 255)),
    )
}

#[test]
fn matrix_is_explicit_about_subset_and_standard_gaps() {
    let matrix = CpuGpuSubset::new().support_matrix();
    assert!(matrix.supports(Capability::ScreenSpaceTriangleDraw));
    assert_eq!(
        matrix.get(Capability::StandardWebGpuAdapter).status,
        SupportStatus::Unsupported
    );

    let unsupported = matrix
        .require(Capability::AsynchronousQueue)
        .expect_err("P0 queue is synchronous");
    assert_eq!(unsupported.capability, Capability::AsynchronousQueue);
    assert!(!unsupported.reason.is_empty());
}

#[test]
fn parallel_descriptor_matches_the_build_capability() {
    let adapter = CpuGpuSubset::new().request_adapter();
    let descriptor = DeviceDescriptor::new(Extent2d::new(2, 2)).with_parallel(true);
    let result = adapter.request_device(descriptor);
    if cfg!(feature = "parallel") {
        assert!(result.is_ok());
    } else {
        assert!(result.is_err());
        assert!(
            !adapter
                .support_matrix()
                .supports(Capability::DeterministicTileParallelism)
        );
    }
}

#[test]
fn records_submits_and_reads_back_clear_and_triangle() {
    let mut device = device(8, 8);
    let selected = PixelCoord::new(2, 2);
    let mut encoder = device.create_command_encoder();
    encoder.clear(Rgba8::BLACK, 1.0).unwrap();
    encoder.select_pixel(selected).unwrap();
    encoder.draw_triangle(red_triangle()).unwrap();

    let report = device
        .queue_mut()
        .submit(encoder.finish().unwrap())
        .unwrap();
    assert_eq!(report.serial, 1);
    assert_eq!(report.triangle_count, 1);
    assert!(report.selected_pixel_recorded);
    assert_eq!(device.queue().last_completed_serial(), Some(1));

    let framebuffer = device.queue().readback_framebuffer().unwrap();
    assert_eq!(
        framebuffer.color_at(selected),
        Some(Rgba8::new(255, 0, 0, 255))
    );
    assert_eq!(device.queue().readback_rgba8().unwrap().len(), 8 * 8 * 4);

    let history = device
        .queue()
        .selected_pixel_history()
        .expect("selected pixel history");
    assert_eq!(history.pixel, selected);
    assert!(
        history
            .events
            .iter()
            .any(|event| event.outcome == PixelEventOutcome::Written)
    );
}

#[test]
fn clear_only_submission_is_valid_and_replaces_previous_output() {
    let mut device = device(3, 2);
    let clear = Rgba8::new(5, 10, 15, 255);
    let mut encoder = device.create_command_encoder();
    encoder.clear(clear, 0.75).unwrap();

    let report = device
        .queue_mut()
        .submit(encoder.finish().unwrap())
        .unwrap();
    assert_eq!(report.triangle_count, 0);
    assert!(
        device
            .queue()
            .readback_color()
            .unwrap()
            .iter()
            .all(|pixel| *pixel == clear)
    );
}

#[test]
fn command_encoder_rejects_operations_outside_p0_contract() {
    let device = device(4, 4);
    let mut encoder = device.create_command_encoder();
    assert_eq!(
        encoder.draw_triangle(red_triangle()).unwrap_err(),
        CommandEncodingError::DrawBeforeClear
    );
    encoder.clear(Rgba8::BLACK, 1.0).unwrap();
    assert_eq!(
        encoder.clear(Rgba8::WHITE, 1.0).unwrap_err(),
        CommandEncodingError::DuplicateClear
    );
    assert!(matches!(
        encoder.select_pixel(PixelCoord::new(4, 0)),
        Err(CommandEncodingError::SelectedPixelOutOfBounds { .. })
    ));
}

#[test]
fn device_exposes_shader_validation_without_claiming_execution() {
    let device = device(2, 2);
    let module = device
        .create_shader_module(
            "@compute @workgroup_size(1) fn cs_main(@builtin(global_invocation_id) id: vec3<u32>) { _ = id; }",
        )
        .expect("valid WGSL");
    assert!(
        module
            .info()
            .entry_point(ShaderStage::Compute, "cs_main")
            .is_some()
    );
    assert!(!device.support_matrix().supports(Capability::WgslExecution));
}
