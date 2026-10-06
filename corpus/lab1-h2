#include <stdio.h>

//Задача 2. Вариант 5

int main(void) {
    int a, b, c;

    printf("Введите номер билета\n");

    if (scanf("%d", &a) != 1 || a < 100000 || a > 999999){
        printf("Ошибка ввода!\n");
        return 1;
    }

    b = a / 100000 + a / 10000 % 10 + a / 1000 % 10;

    c = a % 10 + a / 10 % 10 + a / 100 % 10;

    if (b == c) {
        printf("БИЛЕТ СЧАСТЛИВЫЙ!!! (✿^‿^)\n");
    } else {
        printf("Вам не повезло (｡ŏ﹏ŏ)\n");
    }

    return 0;
}