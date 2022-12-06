#version 460

#extension GL_GOOGLE_include_directive : enable
#extension GL_EXT_nonuniform_qualifier : require

#include "shared.glsl"


// Vertex attributes
layout (location = 0) in vec2 a_pos;
layout (location = 1) in vec2 a_tc;
layout (location = 2) in vec4 a_srgba;


//To fragment shader
layout (location = 0) out vec4 v_rgba_gamma;
layout (location = 1) out vec2 v_tc;

//push constants block
layout( push_constant ) uniform push{
  ResHandle tex;
  ResHandle sam;
  ResHandle pad0[2];
  vec2 screen_size;
  vec2 pad1;
} Push;

layout(set = 1, binding = 0) uniform writeonly image2D global_images_2d[];


void main(){
    gl_Position = vec4(
                      (2.0 * a_pos.x / Push.screen_size.x) - 1.0,
                      (2.0 * a_pos.y / Push.screen_size.y) - 1.0,
                      0.0,
                      1.0);
    v_rgba_gamma = a_srgba;
    v_tc = a_tc;
}
