#version 300 es

in vec4 position;
out vec2 screenPos;
out vec2 screenPosF;

void main() {
    screenPos = vec2(max((position.x * 0.5 + 0.5) * 256.0 - 0.1, 0.0), max(position.y - 0.1, 0.0));
    screenPosF = vec2(position.x * 0.5 + 0.5, position.y / 192.0);
    // Draw upside down, since opengl starts fbo texture bottom left
    gl_Position = vec4(position.x, position.y / 192.0 * 2.0 - 1.0, 0.0, 1.0);
}
