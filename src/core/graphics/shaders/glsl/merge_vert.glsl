#version 300 es

in vec4 position;
out vec2 texCoords;

void main() {
    texCoords = position.zw;
    gl_Position = vec4(position.xy, 0.0, 1.0);
}
