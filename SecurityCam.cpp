#include <opencv2/opencv.hpp>

int main() {
    cv::Mat image = cv::imread("image.png", cv::IMREAD_ANYCOLOR);
    cv::namedWindow("Image", cv::WINDOW_AUTOSIZE);
    cv::imshow("Image", image);

    cv::waitKey(0);

    return 0;
}
