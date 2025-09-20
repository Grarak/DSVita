#version 300 es

in vec4 position;
in vec4 color;
out vec4 fragColor;

void main() {
    fragColor = color;
    gl_Position = vec4(position.x / 480.0 - 1.0, -position.y / 272.0 + 1.0, 0.0, 1.0);
}
