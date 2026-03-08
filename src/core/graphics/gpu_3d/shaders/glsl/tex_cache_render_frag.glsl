precision highp float;
precision highp int;

uniform float polygonAttrsF;
uniform float texImageParamF;

uniform sampler2D tex;

in vec4 oColor;
in vec2 oTexCoords;
in vec4 oTexModeWeights;

layout (location = 0) out vec4 color;

void main() {
    int polygonAttrs = floatBitsToInt(polygonAttrsF);
    bool noTex = (polygonAttrs & 1) != 0;
    vec4 texColor;
    if (!noTex) {
        vec4 weights = round(oTexModeWeights);
        vec2 texCoords = oTexCoords * (1.0 - weights.xz);
        vec2 texCoordsFrac = fract(oTexCoords);
        vec2 texCoordsMod = fract(floor(oTexCoords) / weights.yw) * weights.yw;
        texCoordsFrac = texCoordsFrac * (1.0 - 2.0 * texCoordsMod) + texCoordsMod;
        texCoords += texCoordsFrac * weights.xz;

        texColor = texture(tex, texCoords);
        if (texColor.a == 0.0) {
            discard;
        }

        int mode = (polygonAttrs >> 4) & 0x3;
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

#ifdef TRANSLUCENT
    if (color.a >= 0.99) {
        discard;
    }
#else
    if (color.a < 0.99) {
        bool transNewDepth = ((polygonAttrs >> 11) & 1) != 0;
        if (transNewDepth) {
            color.a = 0.0;
        } else {
            discard;
        }
    }
#endif

#ifdef W_DEPTH_BUFFER
    gl_FragDepth = 1.0 / gl_FragCoord.w / 4096.0;
#endif
}
