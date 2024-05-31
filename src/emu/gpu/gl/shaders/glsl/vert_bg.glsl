#version 300 es

layout(location = 0) in vec4 vPosition;
out vec2 screenPos;

void main() {
    screenPos = vec2((vPosition.x * 0.5 + 0.5) * 256.0 - 0.1, vPosition.y - 0.1);
    gl_Position = vec4(vPosition.x, 1.0 - vPosition.y / 192.0 * 2.0, 0.0, vPosition.w);
}
