#version 300 es

in vec4 position;
in vec4 color;
in vec2 texCoords;

out vec4 oColor;
out vec2 oTexCoords;

void main() {
    oColor = color;
    oTexCoords = texCoords;
    gl_Position = vec4(position.x / (16.0 * 256.0), position.y / (16.0 * 192.0), position.zw / 4096.0);
}
