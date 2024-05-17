#version 300 es

precision highp float;
precision highp int;

layout(location = 0) out vec4 color;
in vec2 screenPos;

void main() {
    color = vec4(screenPos.x / 255.0, screenPos.y / 191.0, 0.0, 1.0);
}
