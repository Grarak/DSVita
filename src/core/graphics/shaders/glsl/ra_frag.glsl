#version 300 es

precision mediump float;

layout(location = 0) out vec4 color;

uniform sampler2D tex;
in vec2 texCoords;
flat in float texFactor;

uniform float alpha;

void main() {
    vec4 texColor = texture(tex, texCoords);
    color = vec4(texColor.rgb * texFactor + vec3(0.2, 0.2, 0.2) * (1.0 - texFactor), alpha);
}
