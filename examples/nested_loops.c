#include <stdio.h>

int main(void) {
    int i, j;
    for (i = 0; i < 10; i++) {
        while (i > 5) {
            printf("%d", i);
            i--;
        }
        j = i * 2;
        printf("j = %d", j);
    }
    return 0;
}
