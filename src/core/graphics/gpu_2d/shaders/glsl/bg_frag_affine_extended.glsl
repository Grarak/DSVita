vec4 drawAffine(int x, int y, int bgNum) {
    int size = int(affineDims.x);

    ivec2 coords = calculateAffineCoords(x, y, bgNum);

    bool wrap = ((bgCnt >> 13) & 1) != 0;
    if (wrap) {
        coords.x &= size - 1;
        coords.y &= size - 1;
    } else if (coords.x < 0 || coords.x >= size || coords.y < 0 || coords.y >= size) {
        discard;
    }

    int screenAddr = (((dispCnt >> 27) & 0x7) * 64 + ((bgCnt >> 8) & 0x1F) * 2) * 1024;
    int charAddr = (((dispCnt >> 24) & 0x7) * 64 + ((bgCnt >> 2) & 0xF) * 16) * 1024;

    int xBlockNum = coords.x >> 3;
    int xInBlock = coords.x & 7;
    int yBlockNum = coords.y >> 3;
    int yInBlock = coords.y & 7;

    screenAddr += (yBlockNum * (size / 8) + xBlockNum) * 2;
    int screenEntry = readBg16Aligned(screenAddr);

    int isHFlip = (screenEntry >> 10) & 1;
    int isVFlip = (screenEntry >> 11) & 1;

    xInBlock = abs(isHFlip * 7 - xInBlock);
    yInBlock = abs(isVFlip * 7 - yInBlock);

    charAddr += (screenEntry & 0x3FF) * 64 + yInBlock * 8 + xInBlock;

    int palAddr = readBg8(charAddr);
    if (palAddr == 0) {
        discard;
    }
    palAddr *= 2;

    bool useExtPal = (dispCnt & (1 << 30)) != 0;
    if (useExtPal) {
        palAddr += bgNum * 8192 + ((screenEntry & 0xF000) >> 3);
        int color = readExtPal16Aligned(palAddr);
        return vec4(normRgb5(color), 1.0);
    } else {
        int color = readPal16Aligned(palAddr);
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

    color = drawAffine(x, y, bgNum);
    setPrio();
}
