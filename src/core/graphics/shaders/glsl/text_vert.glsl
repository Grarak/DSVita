#version 300 es

in vec4 position;
out vec2 texCoords;

void main() {
    texCoords = position.zw;
    gl_Position = vec4(position.x / 550.0 + 0.6, -position.y / 300.0 + 0.925, 0.0, 1.0);
}
