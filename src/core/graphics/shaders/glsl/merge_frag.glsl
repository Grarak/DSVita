#version 300 es

precision mediump float;

layout(location = 0) out vec4 color;

uniform sampler2D tex;
in vec2 texCoords;

uniform float alpha;

void main() {
    color = vec4(texture(tex, texCoords).rgb, alpha);
}
