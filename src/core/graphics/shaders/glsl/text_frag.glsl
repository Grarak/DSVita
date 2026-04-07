#version 300 es

precision mediump float;

layout(location = 0) out vec4 color;

uniform sampler2D tex;
in vec2 texCoords;

uniform float alpha;

void main() {
    float a = texture(tex, texCoords).r;
    if (a <= 0.0) {
        discard;
    }
    color = vec4(1.0, 1.0, 1.0, a * alpha);
}
