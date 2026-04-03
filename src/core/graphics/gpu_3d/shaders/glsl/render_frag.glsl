precision highp float;
precision highp int;

uniform float polygonAttrsF;
uniform float texImageParamF;

uniform sampler2D tex;

in vec4 oColor;
in vec2 oTexCoords;

layout (location = 0) out vec4 color;

const vec2 texModLookup[3] = vec2[3](
    vec2(2.0, 1.0), vec2(1.0, 2.0), vec2(2.0, 2.0)
);

void main() {
    vec4 texColor = texture(tex, oTexCoords);

    int polygonAttrs = floatBitsToInt(polygonAttrsF);
    int texImageParam = floatBitsToInt(texImageParamF);

    int texFormat = (texImageParam >> 26) & 0x7;
    if (texFormat != 0) {
        if (texColor.a == 0.0) {
            discard;
        }

        int mode = polygonAttrs & 0x3;
        switch (mode) {
            case 0:
            case 2:
                color = texColor * oColor;
                break;
            case 1:
            case 3:
                color.rgb = texColor.rgb * texColor.a + oColor.rgb * (1.0 - texColor.a);
                color.a = oColor.a;
                break;
        }
    } else {
        color = oColor;
    }

#ifdef W_DEPTH_BUFFER
    gl_FragDepth = 1.0 / gl_FragCoord.w / 4096.0;
#endif
}
