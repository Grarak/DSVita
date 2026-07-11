precision highp float;
precision highp int;

uniform float polygonAttrsF;
uniform float texImageParamF;
uniform float toonHighlight;
uniform vec3 toonTable[32];

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
    int mode = polygonAttrs & 0x3;
    if (texFormat != 0) {
        if (texColor.a == 0.0) {
            discard;
        }

        switch (mode) {
            case 0:
                color = texColor * oColor;
                break;
            case 1:
            case 3:
                color.rgb = texColor.rgb * texColor.a + oColor.rgb * (1.0 - texColor.a);
                color.a = oColor.a;
                break;
            case 2: {
                vec3 toon = toonTable[int(min(oColor.r * 31.0 + 0.5, 31.0))];
                if (toonHighlight != 0.0) {
                    color.rgb = min(texColor.rgb * oColor.rgb + toon, 1.0);
                } else {
                    color.rgb = texColor.rgb * toon;
                }
                color.a = texColor.a * oColor.a;
                break;
            }
        }
    } else if (mode == 2) {
        vec3 toon = toonTable[int(min(oColor.r * 31.0 + 0.5, 31.0))];
        if (toonHighlight != 0.0) {
            color.rgb = min(oColor.rgb + toon, 1.0);
        } else {
            color.rgb = toon;
        }
        color.a = oColor.a;
    } else {
        color = oColor;
    }

#ifdef W_DEPTH_BUFFER
    float depth = 1.0 / gl_FragCoord.w / 4096.0;
    // The DS depth-equal test passes within a margin (0xFF of 0xFFFFFF for w-buffering);
    // bias equal-test polygons towards the viewer and rely on LEQUAL to emulate that.
    if ((polygonAttrs & 0x80) != 0) {
        depth -= float(0xFF) / float(0xFFFFFF);
    }
    gl_FragDepth = depth;
#endif
}
