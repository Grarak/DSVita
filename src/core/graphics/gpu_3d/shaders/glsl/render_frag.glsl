#version 300 es

precision highp int;
precision highp float;

uniform sampler2D tex;
uniform sampler2D palTex;

in vec3 oColor;
in vec2 oTexCoords;
in float oPolygonIndex;

layout(location = 0) out vec4 color;

struct Polygon {
    int texImageParam;
    int palAddr;
};

uniform PolygonUbo {
    Polygon polygons[2048];
};

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
    float y = float(addrY) / 95.0;
    vec4 value = texture(palTex, vec2(x, y));
    int entry = addr & 2;
    return int(value[entry] * 255.0) | (int(value[entry + 1] * 255.0) << 8);
}

vec3 normRgb5(int color) {
    return vec3(float(color & 0x1F), float((color >> 5) & 0x1F), float((color >> 10) & 0x1F)) / 31.0;
}

void a3i5Tex(int index, int addrOffset, int s, int t, int sizeS) {
    int addr = addrOffset + t * sizeS + s;

    int palIndex = readTex8(addr);
    if (palIndex == 0) {
        discard;
    }

    int palOffset = polygons[index].palAddr << 4;
    int tex = readPal16Aligned(palOffset + (palIndex & 0x1F) * 2);
    float alpha = float((palIndex >> 5) & 0x3) / 7.0;
    color = vec4(normRgb5(tex), alpha);
}

void pal4Tex(int index, int addrOffset, int s, int t, int sizeS, bool transparent0) {
    int addr = addrOffset + (t * sizeS + s) / 4;

    int palIndex = readTex8(addr);
    if (transparent0 && palIndex == 0) {
        discard;
    }
    palIndex = (palIndex >> ((s % 4) * 2)) & 0x03;

    int palOffset = polygons[index].palAddr << 3;
    int tex = readPal16Aligned(palOffset + palIndex * 2);
    color = vec4(normRgb5(tex), 1.0);
}

void pal16Tex(int index, int addrOffset, int s, int t, int sizeS, bool transparent0) {
    int addr = addrOffset + (t * sizeS + s) / 2;

    int palIndex = readTex8(addr);
    if (transparent0 && palIndex == 0) {
        discard;
    }
    palIndex = (palIndex >> ((s % 2) * 4)) & 0x0F;

    int palOffset = polygons[index].palAddr << 4;
    int tex = readPal16Aligned(palOffset + palIndex * 2);
    color = vec4(normRgb5(tex), 1.0);
}

void pal256Tex(int index, int addrOffset, int s, int t, int sizeS, bool transparent0) {
    int addr = addrOffset + (t * sizeS + s);

    int palIndex = readTex8(addr);
    if (transparent0 && palIndex == 0) {
        discard;
    }

    int palOffset = polygons[index].palAddr << 4;
    int tex = readPal16Aligned(palOffset + palIndex * 2);
    color = vec4(normRgb5(tex), 1.0);
}

void a5i3Tex(int index, int addrOffset, int s, int t, int sizeS) {
    int addr = addrOffset + t * sizeS + s;

    int palIndex = readTex8(addr);
    if (palIndex == 0) {
        discard;
    }

    int palOffset = polygons[index].palAddr << 4;
    int tex = readPal16Aligned(palOffset + (palIndex & 0x07) * 2);
    float alpha = float((palIndex >> 3) & 0x1F) / 31.0;
    color = vec4(normRgb5(tex), alpha);
}

void directTex(int addrOffset, int s, int t, int sizeS) {
    int addr = (addrOffset + t * sizeS + s) * 2;
    int tex = readTex16Aligned(addr);
    if (tex == 0) {
        discard;
    }
    color = vec4(normRgb5(tex), 1.0);
}

void main() {
    int polygonIndex = int(oPolygonIndex);
    int texImageParam = polygons[polygonIndex].texImageParam;

    int addrOffset = (texImageParam & 0xFFFF) << 3;
    int sizeS = 8 << ((texImageParam >> 20) & 0x7);
    int sizeT = 8 << ((texImageParam >> 23) & 0x7);
    int s = int(oTexCoords.s);
    int t = int(oTexCoords.t);

    bool repeatS = ((texImageParam >> 16) & 0x1) == 1;
    bool repeatT = ((texImageParam >> 17) & 0x1) == 1;
    if (repeatS) {
        bool flip = ((texImageParam >> 18) & 0x1) == 1;
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
        bool flip = ((texImageParam >> 19) & 0x1) == 1;
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

    int texFmt = (texImageParam >> 26) & 0x7;

    switch (texFmt) {
        case 0: {
            color = vec4(oColor, 1.0);
            break;
        }
        case 1: {
            a3i5Tex(polygonIndex, addrOffset, int(s), int(t), sizeS);
            break;
        }
        case 2: {
            bool transparent0 = ((texImageParam >> 29) & 0x1) == 1;
            pal4Tex(polygonIndex, addrOffset, int(s), int(t), sizeS, transparent0);
            break;
        }
        case 3: {
            bool transparent0 = ((texImageParam >> 29) & 0x1) == 1;
            pal16Tex(polygonIndex, addrOffset, int(s), int(t), sizeS, transparent0);
            break;
        }
        case 4: {
            bool transparent0 = ((texImageParam >> 29) & 0x1) == 1;
            pal256Tex(polygonIndex, addrOffset, int(s), int(t), sizeS, transparent0);
            break;
        }
        case 6: {
            a5i3Tex(polygonIndex, addrOffset, int(s), int(t), sizeS);
            break;
        }
        case 7: {
            directTex(addrOffset, int(s), int(t), sizeS);
            break;
        }
    }
}
