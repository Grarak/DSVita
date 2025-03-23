#version 300 es

precision highp int;
precision highp float;

uniform sampler2D tex;
uniform sampler2D palTex;
uniform sampler2D attrTex;

in vec3 oColor;
in vec2 oTexCoords;
in float oPolygonIndex;

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

vec3 normRgb5(int color) {
    return vec3(float(color & 0x1F), float((color >> 5) & 0x1F), float((color >> 10) & 0x1F)) / 31.0;
}

void a3i5Tex(int palAddr, int addrOffset, int s, int t, int sizeS) {
    int addr = addrOffset + t * sizeS + s;

    int palIndex = readTex8(addr);
    if (palIndex == 0) {
        discard;
    }

    int palOffset = palAddr << 4;
    int tex = readPal16Aligned(palOffset + (palIndex & 0x1F) * 2);
    float alpha = float((palIndex >> 5) & 0x3) / 7.0;
    color = vec4(normRgb5(tex), alpha);
}

void pal4Tex(int palAddr, int addrOffset, int s, int t, int sizeS, bool transparent0) {
    int addr = addrOffset + (t * sizeS + s) / 4;

    int palIndex = readTex8(addr);
    palIndex = (palIndex >> ((s % 4) * 2)) & 0x03;
    if (transparent0 && palIndex == 0) {
        discard;
    }

    int palOffset = palAddr << 3;
    int tex = readPal16Aligned(palOffset + palIndex * 2);
    color = vec4(normRgb5(tex), 1.0);
}

void pal16Tex(int palAddr, int addrOffset, int s, int t, int sizeS, bool transparent0) {
    int addr = addrOffset + (t * sizeS + s) / 2;

    int palIndex = readTex8(addr);
    palIndex = (palIndex >> ((s & 0x1) * 4)) & 0x0F;
    if (transparent0 && palIndex == 0) {
        discard;
    }

    int palOffset = palAddr << 4;
    int tex = readPal16Aligned(palOffset + palIndex * 2);
    color = vec4(normRgb5(tex), 1.0);
}

void pal256Tex(int palAddr, int addrOffset, int s, int t, int sizeS, bool transparent0) {
    int addr = addrOffset + (t * sizeS + s);

    int palIndex = readTex8(addr);
    if (transparent0 && palIndex == 0) {
        discard;
    }

    int palOffset = palAddr << 4;
    int tex = readPal16Aligned(palOffset + palIndex * 2);
    color = vec4(normRgb5(tex), 1.0);
}

void compressed4x4Tex(int palAddr, int addrOffset, int s, int t, int sizeS) {
    int tile = (t / 4) * (sizeS / 4) + (s / 4);
    int addr = addrOffset + (tile * 4 + (t & 0x3));

    int palIndex = readTex8(addr);
    palIndex = (palIndex >> ((s & 0x3) * 2)) & 0x3;

    addr = 0x20000 + (addrOffset & 0x1FFFF) / 2 + (((addrOffset >> 17) == 2) ? 0x10000 : 0);
    int palBase = readTex16Aligned(addr + tile * 2);
    int palOffset = (palAddr << 4) + (palBase & 0x3FFF) * 4;

    int mode = (palBase >> 14) & 0x3;
    switch (mode) {
        case 0: {
            if (palIndex == 3) {
                discard;
            }
            int tex = readPal16Aligned(palOffset + palIndex * 2);
            color = vec4(normRgb5(tex), 1.0);
            break;
        }
        case 1: {
            switch (palIndex) {
                case 2: {
                    int tex = readPal16Aligned(palOffset);
                    vec4 color0 = vec4(normRgb5(tex), 1.0);
                    tex = readPal16Aligned(palOffset + 2);
                    vec4 color1 = vec4(normRgb5(tex), 1.0);
                    color = (color0 + color1) / 2.0;
                    break;
                }
                case 3: {
                    discard;
                }
                default : {
                    int tex = readPal16Aligned(palOffset + palIndex * 2);
                    color = vec4(normRgb5(tex), 1.0);
                    break;
                }
            }
            break;
        }
        case 2: {
            int tex = readPal16Aligned(palOffset + palIndex * 2);
            color = vec4(normRgb5(tex), 1.0);
            break;
        }
        case 3: {
            switch (palIndex) {
                case 2: {
                    int tex = readPal16Aligned(palOffset);
                    vec4 color0 = vec4(normRgb5(tex), 1.0);
                    tex = readPal16Aligned(palOffset + 2);
                    vec4 color1 = vec4(normRgb5(tex), 1.0);
                    color = (color0 * 5.0 + color1 * 3.0) / 8.0;
                    break;
                }
                case 3: {
                    int tex = readPal16Aligned(palOffset);
                    vec4 color0 = vec4(normRgb5(tex), 1.0);
                    tex = readPal16Aligned(palOffset + 2);
                    vec4 color1 = vec4(normRgb5(tex), 1.0);
                    color = (color0 * 3.0 + color1 * 5.0) / 8.0;
                    break;
                }
                default : {
                    int tex = readPal16Aligned(palOffset + palIndex * 2);
                    color = vec4(normRgb5(tex), 1.0);
                    break;
                }
            }
            break;
        }
    }
}

void a5i3Tex(int palAddr, int addrOffset, int s, int t, int sizeS) {
    int addr = addrOffset + t * sizeS + s;

    int palIndex = readTex8(addr);
    if (palIndex == 0) {
        discard;
    }

    int palOffset = palAddr << 4;
    int tex = readPal16Aligned(palOffset + (palIndex & 0x07) * 2);
    float alpha = float((palIndex >> 3) & 0x1F) / 31.0;
    color = vec4(normRgb5(tex), alpha);
}

void directTex(int addrOffset, int s, int t, int sizeS) {
    int addr = addrOffset + (t * sizeS + s) * 2;
    int tex = readTex16Aligned(addr);
    if (tex == 0) {
        discard;
    }
    if ((tex >> 15) == 0) {
        color = vec4(normRgb5(tex), 0.0);
    } else {
        color = vec4(normRgb5(tex), 1.0);
    }
}

void main() {
    int polygonIndex = int(oPolygonIndex);

    polygonIndex <<= 1;
    float x = float(polygonIndex & 0x7F) / 127.0;
    float y = float(polygonIndex >> 7) / 127.0;
    vec4 value = texture(attrTex, vec2(x, y));

    int addrOffset = (int(value[0] * 255.0) | (int(value[1] * 255.0) << 8)) << 3;
    int texImageParam = int(value[2] * 255.0) | (int(value[3] * 255.0) << 8);

    polygonIndex += 1;
    x = float(polygonIndex & 0x7F) / 127.0;
    y = float(polygonIndex >> 7) / 127.0;
    value = texture(attrTex, vec2(x, y));
    int palAddr = int(value[0] * 255.0) | (int(value[1] * 255.0) << 8);
    int polyAttr = int(value[2] * 255.0) | (int(value[3] * 255.0) << 8);

    int sizeS = 8 << ((texImageParam >> 4) & 0x7);
    int sizeT = 8 << ((texImageParam >> 7) & 0x7);
    int s = int(oTexCoords.s);
    int t = int(oTexCoords.t);

    bool repeatS = (texImageParam & 0x1) == 1;
    bool repeatT = ((texImageParam >> 1) & 0x1) == 1;
    if (repeatS) {
        bool flip = ((texImageParam >> 2) & 0x1) == 1;
        if (flip && (s & sizeT) != 0) {
            s = -s;
        }
        s += sizeS;
        s &= sizeS - 1;
    } else if (s < 0) {
        s = 0;
    } else if (s >= sizeS) {
        s = sizeS - 1;
    }

    if (repeatT) {
        bool flip = ((texImageParam >> 3) & 0x1) == 1;
        if (flip && (t & sizeT) != 0) {
            t = -t;
        }
        t += sizeT;
        t &= sizeT - 1;
    } else if (t < 0) {
        t = 0;
    } else if (t >= sizeT) {
        t = sizeT - 1;
    }

    int texFmt = (texImageParam >> 10) & 0x7;

    switch (texFmt) {
        case 0: {
            color = vec4(oColor, 1.0);
            break;
        }
        case 1: {
            a3i5Tex(palAddr, addrOffset, int(s), int(t), sizeS);
            break;
        }
        case 2: {
            bool transparent0 = ((texImageParam >> 13) & 0x1) == 1;
            pal4Tex(palAddr, addrOffset, int(s), int(t), sizeS, transparent0);
            break;
        }
        case 3: {
            bool transparent0 = ((texImageParam >> 13) & 0x1) == 1;
            pal16Tex(palAddr, addrOffset, int(s), int(t), sizeS, transparent0);
            break;
        }
        case 4: {
            bool transparent0 = ((texImageParam >> 13) & 0x1) == 1;
            pal256Tex(palAddr, addrOffset, int(s), int(t), sizeS, transparent0);
            break;
        }
        case 5: {
            compressed4x4Tex(palAddr, addrOffset, int(s), int(t), sizeS);
            break;
        }
        case 6: {
            a5i3Tex(palAddr, addrOffset, int(s), int(t), sizeS);
            break;
        }
        case 7: {
            directTex(addrOffset, int(s), int(t), sizeS);
            break;
        }
    }

    float alpha = float(polyAttr & 31) / 31.0;
    color.a *= alpha;
}
