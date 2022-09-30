#version 460

#extension GL_GOOGLE_include_directive : enable
#extension GL_EXT_nonuniform_qualifier : require

#include "shared.glsl"

//To fragment shader
layout (location = 0) in vec3 inNormal;
layout (location = 1) in vec3 inColor;
layout (location = 2) in vec3 inUV;


layout (location = 0) out vec4 outFragColor;


//push constants block
layout( push_constant ) uniform ForwardPush;

//Camera UBOs
layout(set = 0, binding = 0) buffer UBO{
  mat4 model_view;
  mat4 projection;
} global_buffers_objects[];
layout(set = 1, binding = 0) uniform writeonly image2D global_images_2d[];



void main(){
     outFragColor = vec4(1.0);
}
