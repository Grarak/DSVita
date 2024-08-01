#version 300 es

in vec4 position;
out vec2 screenPos;

void main() {
    screenPos = vec2(max((position.x * 0.5 + 0.5) * 256.0 - 0.1, 0.0), max(position.y - 0.1, 0.0));
    // Draw upside down, since opengl starts fbo texture bottom left
    gl_Position = vec4(position.x, 1.0 - position.y / 192.0 * 2.0, 0.0, 1.0);
}
