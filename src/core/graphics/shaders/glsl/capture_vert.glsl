#version 300 es

in vec4 position;
out vec2 texCoords;

uniform vec2 sizeScalar;

void main() {
    texCoords = position.zw * sizeScalar;
    gl_Position = vec4(position.xy * sizeScalar, 0.0, 1.0);
}
