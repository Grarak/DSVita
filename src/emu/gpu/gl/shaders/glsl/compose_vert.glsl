#version 300 es

in vec4 position;
out vec2 screenPos;

void main() {
    screenPos = vec2(position.x * 0.5 + 0.5, position.y * 0.5 + 0.5);
    gl_Position = vec4(position.xy, 0.0, 1.0);
}
