#version 300 es

in vec4 coords;
out vec2 texCoords;

void main() {
    texCoords = vec2(coords.zw);
    gl_Position = vec4(coords.xy, 0.0, 1.0);
}
