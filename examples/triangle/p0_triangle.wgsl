struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) color: vec3f,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var positions = array(
        vec2f(0.0, 0.75),
        vec2f(-0.75, -0.65),
        vec2f(0.75, -0.65),
    );
    var colors = array(
        vec3f(1.0, 0.2, 0.2),
        vec3f(0.2, 1.0, 0.3),
        vec3f(0.2, 0.45, 1.0),
    );

    var output: VertexOutput;
    output.position = vec4f(positions[vertex_index], 0.5, 1.0);
    output.color = colors[vertex_index];
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4f {
    return vec4f(input.color, 1.0);
}

