#version 460

#extension GL_GOOGLE_include_directive : enable
#extension GL_EXT_nonuniform_qualifier : require

#include "shared.glsl"

struct Vertex {
    vec3 pos;
    float u;
    vec3 nrm;
    float v;
};

//To fragment shader
layout(location = 0) out vec3 outNormal;
layout(location = 1) out vec3 outColor;
layout(location = 2) out vec2 outUV;
layout(location = 3) out vec3 outPos;

//push constants block
layout(push_constant) uniform push {
    ForwardPush push;
} Push;

//Camera UBOs
layout(set = 0, binding = 0) readonly buffer ubo {
    mat4 model_view;
    mat4 projection;
} Ubo[];

//SimObject buffer
layout(set = 0, binding = 0) readonly buffer SimObjects {
    SimObject objects[];
} objects[];

//VertexBuffer
layout(set = 0, binding = 0) readonly buffer VertexBuffer {
    Vertex vertices[];
} global_buffers_vertex[];

layout(set = 1, binding = 0) uniform writeonly image2D global_images_2d[];

void main() {
    vec3 location = objects[nonuniformEXT(get_index(Push.push.sim))].objects[gl_InstanceIndex].location.xyz;

    Vertex vertex = global_buffers_vertex[nonuniformEXT(get_index(Push.push.vertex))].vertices[gl_VertexIndex];

    vec4 pos = vec4(vertex.pos + location, 1.0);

    outNormal = normalize(vertex.nrm);
    outColor = vec3(0.9, 0.85, 0.89);
    outUV = vec2(vertex.u, vertex.v);

    gl_Position = Ubo[nonuniformEXT(get_index(Push.push.ubo))].projection * Ubo[nonuniformEXT(get_index(Push.push.ubo))].model_view * pos;
    outPos = gl_Position.xyz;
}
