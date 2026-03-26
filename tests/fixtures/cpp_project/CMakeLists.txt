cmake_minimum_required(VERSION 3.14)
project(cpp_project CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

add_executable(main
    main.cpp
    models.cpp
)
