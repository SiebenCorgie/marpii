#version 460

#extension GL_GOOGLE_include_directive : enable
#extension GL_EXT_nonuniform_qualifier : require

#include "shared.glsl"

//To fragment shader
layout (location = 0) in vec4 rgba_gamma;
layout (location = 1) in vec2 v_tc;


layout (location = 0) out vec4 outFragColor;


//push constants block
layout( push_constant ) uniform push{
  ResHandle tex;
  ResHandle sam;
  uvec2 pad0;
  vec2 screen_size;
  float gamma;
  float pad1;
} Push;

layout(set = 2, binding = 0) uniform texture2D global_sampled_2d[];
layout(set = 3, binding = 0) uniform sampler global_sampler[];

// 0-255 sRGB  from  0-1 linear
vec3 srgb_from_linear(vec3 rgb) {
    bvec3 cutoff = lessThan(rgb, vec3(0.0031308));
    vec3 lower = rgb * vec3(3294.6);
    vec3 higher = vec3(269.025) * pow(rgb, vec3(1.0 / 2.4)) - vec3(14.025);
    return mix(higher, lower, vec3(cutoff));
}

// 0-255 sRGBA  from  0-1 linear
vec4 srgba_from_linear(vec4 rgba) {
    return vec4(srgb_from_linear(rgba.rgb), 255.0 * rgba.a);
}

// 0-1 gamma  from  0-1 linear
vec4 gamma_from_linear_rgba(vec4 linear_rgba) {
    return vec4(srgb_from_linear(linear_rgba.rgb) / 255.0, linear_rgba.a);
}

// srgb to linear
// taken from: https://github.com/MatchaChoco010/egui-winit-ash-integration/blob/main/src/shaders/src/vert.vert
vec3 srgb_to_linear(vec3 srgb) {
    bvec3 cutoff = lessThan(srgb, vec3(0.04045));
    vec3 lower = srgb / vec3(12.92);
    vec3 higher = pow((srgb + vec3(0.055)) / vec3(1.055), vec3(2.4));
    return mix(higher, lower, cutoff);
}

vec3 srgb_gamma_from_linear(vec3 rgb) {
    bvec3 cutoff = lessThan(rgb, vec3(0.0031308));
    vec3 lower = rgb * vec3(12.92);
    vec3 higher = vec3(1.055) * pow(rgb, vec3(1.0 / 2.4)) - vec3(0.055);
    return mix(higher, lower, vec3(cutoff));
}

void main() {

    if (!is_valid(Push.tex) || !is_valid(Push.sam)){
        outFragColor = vec4(1.0, 0.0, 0.0, 1.0);
        return;
    }

    //outFragColor = vec4(0.0, 1.0, 0.0, 1.0);
    //return;
    
    vec4 texval = texture(sampler2D(global_sampled_2d[get_index(Push.tex)], global_sampler[get_index(Push.sam)]), v_tc);


    // The texture is set up with `SRGB8_ALPHA8`

    vec4 texture_in_gamma = vec4(srgb_gamma_from_linear(texval.xyz), texval.w);


    // Multiply vertex color with texture color (in gamma space).
    vec4 rgba = pow(texture_in_gamma * rgba_gamma, vec4(Push.gamma));
    outFragColor = rgba;
}
