use glgpu_shader::{
    ShaderDiagnosticKind, ShaderStage, parse_and_validate_wgsl, parse_and_validate_wgsl_with_label,
};

#[test]
fn validates_vertex_and_fragment_entry_points() {
    let module = parse_and_validate_wgsl(
        r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec3<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 0.5),
        vec2<f32>(-0.5, -0.5),
        vec2<f32>(0.5, -0.5),
    );
    var output: VertexOutput;
    output.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    output.color = vec3<f32>(1.0, 0.25, 0.0);
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(input.color, 1.0);
}
"#,
    )
    .expect("minimal render module should validate");

    assert!(
        module
            .info()
            .entry_point(ShaderStage::Vertex, "vs_main")
            .is_some()
    );
    assert!(
        module
            .info()
            .entry_point(ShaderStage::Fragment, "fs_main")
            .is_some()
    );
}

#[test]
fn validates_compute_entry_point_and_workgroup_size() {
    let module = parse_and_validate_wgsl(
        r#"
@compute @workgroup_size(8, 4, 1)
fn cs_main(@builtin(global_invocation_id) id: vec3<u32>) {
    _ = id;
}
"#,
    )
    .expect("minimal compute module should validate");

    let entry = module
        .info()
        .entry_point(ShaderStage::Compute, "cs_main")
        .expect("compute entry point");
    assert_eq!(entry.workgroup_size, [8, 4, 1]);
}

#[test]
fn parse_error_has_source_span_and_rendered_source() {
    let error = parse_and_validate_wgsl_with_label(
        "broken-parse.wgsl",
        "@compute @workgroup_size(1)\nfn main( {\n}",
    )
    .expect_err("source is malformed");

    assert_eq!(error.kind, ShaderDiagnosticKind::Parse);
    let span = error.span.expect("parse diagnostic should have a span");
    assert!(span.line >= 1);
    assert!(error.source_message.contains("broken-parse.wgsl"));
    assert!(error.source_message.contains('^'));
}

#[test]
fn semantic_error_has_source_span_and_message() {
    let error = parse_and_validate_wgsl_with_label(
        "broken-validation.wgsl",
        r#"
@vertex
fn bad_vertex() -> @builtin(position) vec4<f32> {
    return vec4<u32>(1u, 2u, 3u, 4u);
}
"#,
    )
    .expect_err("return value has the wrong scalar type");

    assert!(matches!(
        error.kind,
        ShaderDiagnosticKind::Parse | ShaderDiagnosticKind::Validation
    ));
    assert!(error.span.is_some());
    assert!(error.source_message.contains("broken-validation.wgsl"));
    assert!(error.source_message.lines().count() > 1);
}

#[test]
fn rejects_optional_capabilities_outside_the_p0_profile() {
    let error = parse_and_validate_wgsl_with_label(
        "optional-f16.wgsl",
        r#"
enable f16;

@compute @workgroup_size(1)
fn cs_main() {
    var value: f16 = 1.0h;
    _ = value;
}
"#,
    )
    .expect_err("P0 must not validate optional f16 capability as supported");

    assert_eq!(error.kind, ShaderDiagnosticKind::Validation);
    assert!(
        error.message.contains("FLOAT16") || error.source_message.contains("FLOAT16"),
        "unexpected optional-capability diagnostic: {error:?}"
    );
}
