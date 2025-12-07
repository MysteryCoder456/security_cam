#include <opencv2/opencv.hpp>

int main() {
    cv::VideoCapture cap(0);
    if (!cap.isOpened()) {
        std::cout << "Couldn't open camera." << std::endl;
        return 1;
    }

    // Detect face
    cv::CascadeClassifier faceCascade(
        "haarcascades/haarcascade_frontalface_default.xml");
    if (faceCascade.empty()) {
        std::cout << "Couldn't load face cascade classifier" << std::endl;
        return 1;
    }

    cv::namedWindow("Camera Feed");
    while (true) {
        cv::Mat frame;
        cap >> frame;

        if (frame.empty()) {
            std::cout << "Unable to read frame from camera" << std::endl;
            break;
        }

        // Detect faces
        cv::Mat frameGray;
        cv::cvtColor(frame, frameGray, cv::COLOR_BGR2GRAY);
        // cv::equalizeHist(frameGray, frameGray); // This is a hit or miss
        std::vector<cv::Rect> faces;
        faceCascade.detectMultiScale(frameGray, faces);

        // Highlight detected faces
        for (auto rect : faces) {
            cv::rectangle(frame, rect, cv::Scalar(50, 50, 255), 4, cv::LINE_8);
        }

        // Mirror and display
        cv::Mat mirroredFrame;
        cv::flip(frame, mirroredFrame, 1);
        cv::imshow("Camera Feed", mirroredFrame);

        if (cv::waitKey(1) == (int)'q')
            break;
    }

    return 0;
}
