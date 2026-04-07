#version 300 es

in vec4 position;
out vec2 texCoords;

uniform vec2 dims;

void main() {
    texCoords = position.zw;
    gl_Position = vec4(position.xy / dims, 0.0, 1.0);
}
