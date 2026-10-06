// черновик.c — меню стран (со скриншотов приложения)
#include <stdio.h>

int main()
{ int a;

printf("strana:\n 1.Brasil\n 2.Belarus\n 3.Kanada\n 4.Russia\n 5.ebania\n");
printf("vabirite strana:\n");
scanf_s("%d",&a);

    switch (a) {
       case 1: printf("South America");
       break;
       case 3: printf("North America");
       break;
       case 2:
       case 4: printf("Euroasia");
       break;
       case 5: printf("ebobo");
       break;
       default: printf("Takoy strani net v spiske\n");
       break;
    }
    return 0;
}
