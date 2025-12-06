cmake_minimum_required(VERSION 3.5)
project(SecurityCam)

find_package(OpenCV REQUIRED)
include_directories(${OpenCV_INCLUDE_DIRS})

add_executable(SecurityCam SecurityCam.cpp)
target_link_libraries(SecurityCam ${OpenCV_LIBS})

