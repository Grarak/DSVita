vec4 drawBitmap(int x, int y, int bgNum) {
    int width = int(affineDims.x);
    int height = int(affineDims.y);

    ivec2 coords = calculateAffineCoords(x, y, bgNum);

    bool wrap = (bgCnt & (1 << 13)) != 0;
    if (wrap) {
        coords.x &= width - 1;
        coords.y &= height - 1;
    } else if (coords.x < 0 || coords.x >= width || coords.y < 0 || coords.y >= height) {
        discard;
    }

    int dataBase = (bgCnt << 6) & 0x7C000;
    bool usePal = (bgCnt & (1 << 2)) == 0;
    if (usePal) {
        int palAddr = readBg8(dataBase + coords.y * width + coords.x);
        if (palAddr == 0) {
            discard;
        }
        palAddr *= 2;

        int color = readPal16Aligned(palAddr);
        return vec4(normRgb5(color), 1.0);
    } else {
        int color = readBg16Aligned(dataBase + (coords.y * width + coords.x) * 2);
        if ((color & (1 << 15)) == 0) {
            discard;
        }
        return vec4(normRgb5(color), 1.0);
    }
}

void main() {
    int bgNum = int(screenPos.z);

    int winEnabled = int(texture(winTex, screenPosF).x * 255.0);
    if ((winEnabled & (1 << bgNum)) == 0) {
        discard;
    }

    int x = int(screenPos.x);
    int y = int(screenPos.y);

    color = drawBitmap(x, y, bgNum);
    setPrio();
}
