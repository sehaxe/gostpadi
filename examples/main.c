#include <stdio.h>

// Определение времени года по номеру месяца
int main(void) {
    int month;

    printf("Введите номер месяца (1-12): ");
    if (scanf("%d", &month) != 1 || month < 1 || month > 12) {
        printf("Ошибка ввода\n");
        return 1;
    }

    switch (month / 3) {
        case 1: printf("Весна\n"); break;
        case 2: printf("Лето\n"); break;
        case 3: printf("Осень\n"); break;
        default: printf("Зима\n");
    }

    return 0;
}
