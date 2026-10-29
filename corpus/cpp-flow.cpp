#include <iostream>
using namespace std;

int main() {
    double v1{}, v2{};
    cout << "v1: ";
    cin >> v1;
    cout << "v2: ";
    cin >> v2;
    if (v2 > v1) {
        cout << "second faster" << endl;
    } else if (v2 == v1) {
        cout << "same" << endl;
    } else {
        cout << "first faster" << endl;
    }
    return 0;
}
