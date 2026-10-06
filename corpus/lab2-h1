#include <stdio.h>

//Задача 1. Вариант 5
//Население города ежегодно увеличивается на 1/n наличного состава жителей,
//где n - натуральное число. Через сколько лет население города утроится?

#define TARGET 3 //во сколько раз должно вырасти население

int main(void) {
    float n;
    unsigned int y;
    double population;
    int c;
    int ok = 0;

    while (!ok) {
        printf("Введите коэффициент: ");
        ok = (scanf("%f", &n) == 1) && (n > 0);

        if (!ok){
            while ((c = getchar()) != '\n' && c != EOF); //вычерпать плохой ввод
            printf("Ошибка: нужно положительное число. Повторите ввод.\n");
        }
    }

    y = 0;
    population = 1;

    while (population < TARGET){
        population = population * (1 + 1/n); //за год население умножается на (1 + 1/n)
        y++;
    }

    printf("Требуется %u лет\n", y);

    return 0;
}
