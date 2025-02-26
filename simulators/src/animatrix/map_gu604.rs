use super::Row;

pub const GU604: [Row; 64] = [
    Row(0x01, 7, 37, 1),
    Row(0x01, 7 + 38, 39, 0),
    Row(0x01, 7 + 77, 38, 1),
    Row(0x01, 7 + 115, 39, 0),
    Row(0x01, 7 + 154, 38, 1),
    Row(0x01, 7 + 192, 39, 0),
    Row(0x01, 7 + 231, 38, 1),
    Row(0x01, 7 + 269, 39, 0),
    Row(0x01, 7 + 308, 38, 1),
    Row(0x01, 7 + 346, 39, 0),
    Row(0x01, 7 + 385, 38, 1),
    Row(0x01, 7 + 423, 38, 1),
    Row(0x01, 7 + 461, 37, 2),
    Row(0x01, 7 + 498, 37, 2),
    Row(0x01, 7 + 535, 36, 3),
    Row(0x01, 7 + 571, 36, 3),
    Row(0x01, 7 + 607, 21, 4), // needs join
    //
    Row(0x74, 7, 14, 24), // adds to end of previous
    Row(0x74, 7 + 15, 35, 4),
    Row(0x74, 7 + 50, 34, 5),
    Row(0x74, 7 + 84, 34, 5),
    Row(0x74, 7 + 118, 33, 6),
    Row(0x74, 7 + 151, 33, 6),
    Row(0x74, 7 + 184, 32, 7),
    Row(0x74, 7 + 216, 32, 7),
    Row(0x74, 7 + 248, 31, 8),
    Row(0x74, 7 + 279, 31, 8),
    Row(0x74, 7 + 310, 30, 9),
    Row(0x74, 7 + 340, 30, 9),
    Row(0x74, 7 + 370, 29, 10),
    Row(0x74, 7 + 399, 29, 10),
    Row(0x74, 7 + 428, 28, 11),
    Row(0x74, 7 + 456, 28, 11),
    Row(0x74, 7 + 484, 27, 12),
    Row(0x74, 7 + 511, 27, 12),
    Row(0x74, 7 + 538, 26, 13),
    Row(0x74, 7 + 564, 26, 13),
    Row(0x74, 7 + 590, 25, 14),
    Row(0x74, 7 + 615, 12, 14), // needs join
    //
    Row(0xe7, 7, 12, 25), // adds to end of previous
    Row(0xe7, 7 + 13, 24, 15),
    Row(0xe7, 7 + 37, 24, 15),
    Row(0xe7, 7 + 61, 23, 16),
    Row(0xe7, 7 + 84, 23, 16),
    Row(0xe7, 7 + 107, 22, 17),
    Row(0xe7, 7 + 129, 22, 17),
    Row(0xe7, 7 + 151, 21, 18),
    Row(0xe7, 7 + 172, 21, 18),
    Row(0xe7, 7 + 193, 20, 19),
    Row(0xe7, 7 + 213, 20, 19),
    Row(0xe7, 7 + 233, 19, 20),
    Row(0xe7, 7 + 252, 19, 20),
    Row(0xe7, 7 + 271, 18, 21),
    Row(0xe7, 7 + 289, 18, 21),
    Row(0xe7, 7 + 307, 17, 22),
    Row(0xe7, 7 + 324, 17, 22),
    Row(0xe7, 7 + 341, 16, 23),
    Row(0xe7, 7 + 357, 16, 23),
    Row(0xe7, 7 + 373, 15, 24),
    Row(0xe7, 7 + 388, 15, 24),
    Row(0xe7, 7 + 403, 14, 25),
    Row(0xe7, 7 + 417, 14, 25),
    Row(0xe7, 7 + 431, 13, 26),
    Row(0xe7, 7 + 444, 13, 26)
];
