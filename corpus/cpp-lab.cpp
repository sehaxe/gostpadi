#include <iostream>
#include <string>
using namespace std;

namespace lab {
constexpr int LIMIT = 3;

int sum(int &a, int &b) {
    int s = a + b;
    return s;
}
} // namespace lab

int main() {
    int x{}, y{};
    cin >> x >> y;
    auto total = lab::sum(x, y);
    string name = "lab";
    if (total > lab::LIMIT) {
        cout << name << ": " << static_cast<double>(total) << endl;
    } else {
        cout << "small" << endl;
    }
    return 0;
}
