#version 300 es

precision highp int;
precision highp float;

uniform bool translucentOnly;

uniform sampler2D tex0;
uniform sampler2D tex1;
uniform sampler2D tex2;
uniform sampler2D tex3;
uniform sampler2D tex4;
uniform sampler2D tex5;
uniform sampler2D tex6;
uniform sampler2D tex7;
uniform sampler2D tex8;
uniform sampler2D tex9;
uniform sampler2D tex10;
uniform sampler2D tex11;
uniform sampler2D tex12;
uniform sampler2D tex13;
uniform sampler2D tex14;
uniform sampler2D tex15;

struct PolygonAttr {
    int texImageParam;
    int palAddrAttrs;
};

uniform PolygonAttrsUbo {
    PolygonAttr polygonAttrs[8192];
};

in vec3 oColor;
in vec2 oTexCoords;
flat in int oPolygonIndex;
flat in int oTextureIndex;

layout (location = 0) out vec4 color;

void main() {
    PolygonAttr attr = polygonAttrs[oPolygonIndex];

    int texImageParam = attr.texImageParam >> 16;
    int polyAttr = attr.palAddrAttrs >> 16;

    float sizeS = float(8 << ((texImageParam >> 4) & 0x7));
    float sizeT = float(8 << ((texImageParam >> 7) & 0x7));

    float sNorm = oTexCoords.s / sizeS;
    float tNorm = oTexCoords.t / sizeT;

    bool repeatS = (texImageParam & 0x1) == 1;
    bool repeatT = ((texImageParam >> 1) & 0x1) == 1;

    if (repeatS) {
        float sFrac = fract(sNorm);
        bool flip = ((texImageParam >> 2) & 0x1) == 1;
        if (flip && mod(floor(sNorm), 2.0) != 0.0) {
            sNorm = 1.0 - sFrac;
        } else {
            sNorm = sFrac;
        }
    }

    if (repeatT) {
        float tFrac = fract(tNorm);
        bool flip = ((texImageParam >> 3) & 0x1) == 1;
        if (flip && mod(floor(tNorm), 2.0) != 0.0) {
            tNorm = 1.0 - tFrac;
        } else {
            tNorm = tFrac;
        }
    }

    float alphaF = float(polyAttr & 31) / 31.0;

    vec4 texColor;
    switch (oTextureIndex) {
        case 0: {
            texColor = texture(tex0, vec2(sNorm, tNorm));
            break;
        }
        case 1: {
            texColor = texture(tex1, vec2(sNorm, tNorm));
            break;
        }
        case 2: {
            texColor = texture(tex2, vec2(sNorm, tNorm));
            break;
        }
        case 3: {
            texColor = texture(tex3, vec2(sNorm, tNorm));
            break;
        }
        case 4: {
            texColor = texture(tex4, vec2(sNorm, tNorm));
            break;
        }
        case 5: {
            texColor = texture(tex5, vec2(sNorm, tNorm));
            break;
        }
        case 6: {
            texColor = texture(tex6, vec2(sNorm, tNorm));
            break;
        }
        case 7: {
            texColor = texture(tex7, vec2(sNorm, tNorm));
            break;
        }
        case 8: {
            texColor = texture(tex8, vec2(sNorm, tNorm));
            break;
        }
        case 9: {
            texColor = texture(tex9, vec2(sNorm, tNorm));
            break;
        }
        case 10: {
            texColor = texture(tex10, vec2(sNorm, tNorm));
            break;
        }
        case 11: {
            texColor = texture(tex11, vec2(sNorm, tNorm));
            break;
        }
        case 12: {
            texColor = texture(tex12, vec2(sNorm, tNorm));
            break;
        }
        case 13: {
            texColor = texture(tex13, vec2(sNorm, tNorm));
            break;
        }
        case 14: {
            texColor = texture(tex14, vec2(sNorm, tNorm));
            break;
        }
        case 15: {
            texColor = texture(tex15, vec2(sNorm, tNorm));
            break;
        }
        default: {
            color = vec4(oColor, alphaF);
            break;
        }
    }

    if (oTextureIndex < 16) {
        if (texColor.a == 0.0) {
            discard;
        }

        int mode = (polyAttr >> 5) & 0x3;
        switch (mode) {
            case 0: {
                color = texColor * vec4(oColor.rgb, alphaF);
                break;
            }
            default: {
                color = texColor;
                color.a *= alphaF;
                break;
            }
        }
    }

    if (translucentOnly) {
        if (color.a == 1.0) {
            discard;
        }
    } else if (color.a != 1.0) {
        bool transNewDepth = ((polyAttr >> 7) & 1) != 0;
        if (transNewDepth) {
            color.a = 0.0;
        } else {
            discard;
        }
    }
}
