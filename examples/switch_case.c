#include <stdio.h>

int main(void) {
    int month;
    printf("Vvedite nomer mesyatsa (1-12): ");
    if (scanf("%d", &month) != 1 || month < 1 || month > 12) {
        printf("Oshibka vvoda\n");
        return 1;
    }
    switch (month / 3) {
        case 1: printf("Vesna\n"); break;
        case 2: printf("Leto\n"); break;
        case 3: printf("Osen\n"); break;
        default: printf("Zima\n");
    }
    return 0;
}
