#version 300 es

precision highp float;

layout(location = 0) out vec2 color;

uniform sampler2D tex;
in vec2 texCoords;

void main() {
    vec4 texColor = texture(tex, texCoords);
    int a = texColor.a != 0.0 ? 1 : 0;
    texColor.rgb *= 31.0;
    int icolor = int(texColor.r) | (int(texColor.g) << 5) | (int(texColor.b) << 10) | (a << 15);
    color = vec2(float(icolor & 0xFF), float((icolor >> 8) & 0xFF)) / 255.0;
}
