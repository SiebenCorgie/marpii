#version 460

#extension GL_GOOGLE_include_directive : enable
#extension GL_EXT_nonuniform_qualifier : require

#include "shared.glsl"

//To fragment shader
layout (location = 0) in vec3 inNormal;
layout (location = 1) in vec3 inColor;
layout (location = 2) in vec2 inUV;
layout (location = 3) in vec3 inPos;


layout (location = 0) out vec4 outFragColor;


//push constants block
layout( push_constant ) uniform ForwardPush;

//Camera UBOs
layout(set = 0, binding = 0) buffer UBO{
  mat4 model_view;
  mat4 projection;
} global_buffers_objects[];
layout(set = 1, binding = 0) uniform writeonly image2D global_images_2d[];


const vec3 LIGHT_LOCATION = vec3(20.0, 20.0, 20.0);


void main(){

  vec3 L = normalize(LIGHT_LOCATION - inPos);

  float LdotN = clamp(dot(L, inNormal), 0.0001, 1.0);

  vec3 color = inColor * LdotN;
  vec3 gamma_corrected = pow(color, vec3(1.0/2.2));
  outFragColor = vec4(gamma_corrected, 1.0);
}
