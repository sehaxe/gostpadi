#include <stdio.h>

//Задача 4. Вариант 5

int main(void) {

    int status;

    printf("Введите время года\n");
    printf("1.Зима\n");
    printf("2.Весна\n");
    printf("3.Лето\n");
    printf("4.Осень\n");

    if (scanf("%d", &status) != 1 || status <= 0 || status >= 5){
        printf("ТЫ НЕ ПРОЙДЕШЬ!!!!\n");
        return 1;
    }

    switch (status) {
        case 1:
            printf("Декабрь, Январь, Февраль\n");
            break;
        case 2:
            printf("Март, Апрель, Май\n");
            break;
        case 3:
            printf("Июнь, Июль, Август\n");
            break;
        case 4:
            printf("Сентябрь, Октябрь, Ноябрь\n");
            break;
        default:
            printf("Как ты сюда попал, но это очень впечатляет\n");
    }

    return 0;

}