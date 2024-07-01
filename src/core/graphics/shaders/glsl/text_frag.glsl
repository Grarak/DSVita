#version 300 es

precision mediump float;

layout(location = 0) out vec4 color;

uniform sampler2D tex;
in vec2 texCoords;

void main() {
    float alpha = texture(tex, texCoords).r;
    if (alpha <= 0.0) {
        discard;
    }
    color = vec4(1.0, 1.0, 1.0, alpha);
}
