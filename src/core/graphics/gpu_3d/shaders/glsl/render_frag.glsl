#version 300 es

precision highp int;
precision highp float;

uniform bool translucentOnly;

uniform sampler2D tex;
uniform sampler2D palTex;
uniform sampler2D attrTex;

in vec3 oColor;
in vec2 oTexCoords;
in vec2 texImageParamAddr;
in vec2 palPolyAttribAddr;

layout (location = 0) out vec4 color;

int readTex8(int addr) {
    float x = float((addr >> 2) & 0x1FF) / 511.0;
    float y = float(addr >> 11) / 255.0;
    return int(texture(tex, vec2(x, y))[addr & 3] * 255.0);
}

int readTex16Aligned(int addr) {
    int addrX = (addr >> 2) & 0x1FF;
    int addrY = addr >> 11;
    float x = float(addrX) / 511.0;
    float y = float(addrY) / 255.0;
    vec4 value = texture(tex, vec2(x, y));
    int entry = addr & 2;
    return int(value[entry] * 255.0) | (int(value[entry + 1] * 255.0) << 8);
}

int readPal16Aligned(int addr) {
    int addrX = (addr >> 2) & 0x1FF;
    int addrY = addr >> 11;
    float x = float(addrX) / 511.0;
    float y = float(addrY) / 47.0;
    vec4 value = texture(palTex, vec2(x, y));
    int entry = addr & 2;
    return int(value[entry] * 255.0) | (int(value[entry + 1] * 255.0) << 8);
}

int readPal32Aligned(int addr) {
    int addrX = (addr >> 2) & 0x1FF;
    int addrY = addr >> 11;
    float x = float(addrX) / 511.0;
    float y = float(addrY) / 47.0;
    vec4 value = texture(palTex, vec2(x, y)) * 255.0;
    return int(value[0]) | (int(value[1]) << 8) | (int(value[2]) << 16) | (int(value[3]) << 24);
}

vec3 normRgb5(int color) {
    return vec3(float(color & 0x1F), float((color >> 5) & 0x1F), float((color >> 10) & 0x1F)) / 31.0;
}

const vec4 TexelMulLookup[16] = vec4[16](
    vec4(1.0, 0.0, 0.0, 0.0), vec4(0.0, 1.0, 0.0, 0.0), vec4(0.0, 0.0, 1.0, 0.0), vec4(-1.0, 0.0, 0.0, 0.0),
    vec4(1.0, 0.0, 0.0, 0.0), vec4(0.0, 1.0, 0.0, 0.0), vec4(0.5, 0.5, 0.0, 0.0), vec4(-1.0, 0.0, 0.0, 0.0),
    vec4(1.0, 0.0, 0.0, 0.0), vec4(0.0, 1.0, 0.0, 0.0), vec4(0.0, 0.0, 1.0, 0.0), vec4(0.0, 0.0, 0.0, 1.0),
    vec4(1.0, 0.0, 0.0, 0.0), vec4(0.0, 1.0, 0.0, 0.0), vec4(5.0 / 8.0, 3.0 / 8.0, 0.0, 0.0), vec4(3.0 / 8.0, 5.0 / 8.0, 0.0, 0.0)
);

vec4 compressed4x4Tex(int palAddr, int addrOffset, int s, int t, int sizeS) {
    int tile = (t / 4) * (sizeS / 4) + (s / 4);
    int addr = addrOffset + (tile * 4 + (t & 0x3));

    int palIndex = readTex8(addr);
    palIndex = (palIndex >> ((s & 0x3) * 2)) & 0x3;

    addr = 0x20000 + (addrOffset & 0x1FFFF) / 2;
    if ((addrOffset >> 17) == 2) {
        addr += 0x10000;
    }
    int palBase = readTex16Aligned(addr + tile * 2);
    int palOffset = (palAddr << 4) + (palBase & 0x3FFF) * 4;

    int colors01 = readPal32Aligned(palOffset);
    int colors23 = readPal32Aligned(palOffset + 4);
    mat4 colors = mat4(
        vec4(normRgb5(colors01 & 0xFFFF), 1.0),
        vec4(normRgb5(colors01 >> 16), 1.0),
        vec4(normRgb5(colors23 & 0xFFFF), 1.0),
        vec4(normRgb5(colors23 >> 16), 1.0)
    );
    int mode = (palBase >> 14) & 0x3;
    int lookup = palIndex | (mode << 2);
    vec4 weights = TexelMulLookup[lookup];
    if (weights.x < 0.0) {
        discard;
    }
    return vec4((colors * weights).rgb, 1.0);
}

vec4 aXiXTex(int palAddr, int addrOffset, int s, int t, int sizeS, int aBits) {
    int addr = addrOffset + t * sizeS + s;

    int palIndex = readTex8(addr);

    if (palIndex == 0) {
        discard;
    }

    int palOffset = palAddr << 4;
    int colorBits = 8 - aBits;
    int colorMask = (1 << colorBits) - 1;
    int aMask = (1 << aBits) - 1;
    float aMax = float(aMask);
    int tex = readPal16Aligned(palOffset + (palIndex & colorMask) * 2);
    float alpha = float((palIndex >> colorBits) & aMask) / aMax;
    return vec4(normRgb5(tex), alpha);
}

vec4 directTex(int addrOffset, int s, int t, int sizeS) {
    int addr = addrOffset + (t * sizeS + s) * 2;
    int color = readTex16Aligned(addr);
    return vec4(normRgb5(color), (color >> 15) == 0 ? 0.0 : 1.0);
}

vec4 palXTex(int palAddr, int addrOffset, int s, int t, int sizeS, int format, bool transparent0) {
    int addr = addrOffset + ((t * sizeS + s) >> (2 - format));

    int palIndex = readTex8(addr);

    int mask1 = (4 >> format) - 1;
    int mask2 = (4 << ((format * 3) & 6)) - 1;
    palIndex = (palIndex >> ((s & mask1) * (2 << format))) & mask2;
    if (transparent0 && palIndex == 0) {
        discard;
    }

    int palOffset = palAddr << (format == 0 ? 3 : 4);
    int tex = readPal16Aligned(palOffset + palIndex * 2);
    return vec4(normRgb5(tex), 1.0);
}

void main() {
    vec4 texImageParamValue = texture(attrTex, texImageParamAddr) * 255.0;
    vec4 palPolyAttribValue = texture(attrTex, palPolyAttribAddr) * 255.0;

    int addrOffset = (int(texImageParamValue[0]) | (int(texImageParamValue[1]) << 8)) << 3;
    int texImageParam = int(texImageParamValue[2]) | (int(texImageParamValue[3]) << 8);
    int palAddr = int(palPolyAttribValue[0]) | (int(palPolyAttribValue[1]) << 8);

    int sizeS = 8 << ((texImageParam >> 4) & 0x7);
    int sizeT = 8 << ((texImageParam >> 7) & 0x7);
    int s = int(oTexCoords.s);
    int t = int(oTexCoords.t);

    bool repeatS = (texImageParam & 0x1) == 1;
    bool repeatT = ((texImageParam >> 1) & 0x1) == 1;
    if (repeatS) {
        bool flip = ((texImageParam >> 2) & 0x1) == 1;
        if (flip && (s & sizeS) != 0) {
            s = -1 - s;
        }
        s &= sizeS - 1;
    } else if (s < 0) {
        s = 0;
    } else if (s >= sizeS) {
        s = sizeS - 1;
    }

    if (repeatT) {
        bool flip = ((texImageParam >> 3) & 0x1) == 1;
        if (flip && (t & sizeT) != 0) {
            t = -1 - t;
        }
        t &= sizeT - 1;
    } else if (t < 0) {
        t = 0;
    } else if (t >= sizeT) {
        t = sizeT - 1;
    }

    int polyAttr = int(palPolyAttribValue[2]) | (int(palPolyAttribValue[3]) << 8);
    float alphaF = float(polyAttr & 31) / 31.0;

    int texFmt = (texImageParam >> 10) & 0x7;
    vec4 texColor;
    switch (texFmt) {
        case 0: {
            color = vec4(oColor, alphaF);
            break;
        }
        case 1: {
            texColor = aXiXTex(palAddr, addrOffset, int(s), int(t), sizeS, 3);
            break;
        }
        case 5: {
            texColor = compressed4x4Tex(palAddr, addrOffset, int(s), int(t), sizeS);
            break;
        }
        case 6: {
            texColor = aXiXTex(palAddr, addrOffset, int(s), int(t), sizeS, 5);
            break;
        }
        case 7: {
            texColor = directTex(addrOffset, int(s), int(t), sizeS);
            break;
        }
        default: {
            bool transparent0 = ((texImageParam >> 13) & 0x1) == 1;
            texColor = palXTex(palAddr, addrOffset, int(s), int(t), sizeS, texFmt - 2, transparent0);
            break;
        }
    }

    if (texFmt != 0) {
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
