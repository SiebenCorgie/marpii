#version 460

#extension GL_GOOGLE_include_directive : enable
#extension GL_EXT_nonuniform_qualifier : require

#include "shared.glsl"


// Vertex attributes
layout (location = 0) in vec3 inPos;
layout (location = 1) in vec3 inNormal;
layout (location = 2) in vec2 inUV;


//To fragment shader
layout (location = 0) out vec3 outNormal;
layout (location = 1) out vec3 outColor;
layout (location = 2) out vec3 outUV;

//push constants block
layout( push_constant ) uniform push{
  ForwardPush push;
} Push;

//Camera UBOs
layout(set = 0, binding = 0) buffer ubo{
  mat4 model_view;
  mat4 projection;
} Ubo[];

//SimObject buffer
layout(set = 0, binding = 0) buffer SimObjects{
  SimObject objects[];
} objects[];

layout(set = 1, binding = 0) uniform writeonly image2D global_images_2d[];

void main(){

  vec3 location = objects[nonuniformEXT(get_index(Push.push.sim))].objects[gl_InstanceIndex].location.xyz;
  //vec3 location = vec3(0.0);

  vec4 pos = vec4(inPos + location, 1.0);

  gl_Position = Ubo[nonuniformEXT(get_index(Push.push.ubo))].projection * Ubo[nonuniformEXT(get_index(Push.push.ubo))].model_view * pos;
}
