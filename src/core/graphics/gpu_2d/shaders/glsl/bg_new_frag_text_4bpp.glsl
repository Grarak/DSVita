#version 300 es

precision mediump float;
precision mediump int;

layout(location = 0) out vec4 color;
in vec2 block;

void main() {
    color = vec4(block / 255.0, 0.0, 1.0);
}
