# Shoggoth SDK — CMake Build Integration
#
# find_package(shoggoth REQUIRED)
# target_link_libraries(my_app PRIVATE shoggoth::sdk)
#
# Or if building the SDK from source as a subdirectory:
# add_subdirectory(path/to/shoggoth-sdk)
# target_link_libraries(my_app PRIVATE shoggoth_sdk)

cmake_minimum_required(VERSION 3.20)
project(shoggoth_sdk VERSION 0.1.0 LANGUAGES C CXX)

option(SHOGGOTH_BUILD_EXAMPLES "Build example programs" ON)
option(SHOGGOTH_BUILD_TESTS "Build test suite" ON)

# ── Library ────────────────────────────────────────────────────────────────────

add_library(shoggoth_sdk SHARED
    src/shoggoth_sdk_impl.c
)

target_include_directories(shoggoth_sdk
    PUBLIC
        $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}>
        $<INSTALL_INTERFACE:include>
)

target_link_libraries(shoggoth_sdk PUBLIC
    curl
    pthread
)

set_target_properties(shoggoth_sdk PROPERTIES
    VERSION ${PROJECT_VERSION}
    SOVERSION 0
    PUBLIC_HEADER "shoggoth_sdk.h;shoggoth_sdk.hpp"
)

# ── Install ────────────────────────────────────────────────────────────────────

include(GNUInstallDirs)

install(TARGETS shoggoth_sdk
    EXPORT shoggothConfig
    LIBRARY DESTINATION ${CMAKE_INSTALL_LIBDIR}
    PUBLIC_HEADER DESTINATION ${CMAKE_INSTALL_INCLUDEDIR}/shoggoth
)

install(EXPORT shoggothConfig
    NAMESPACE shoggoth::
    DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/shoggoth
)

# ── Example ────────────────────────────────────────────────────────────────────

if(SHOGGOTH_BUILD_EXAMPLES)
    add_executable(shoggoth_example examples/hello_shoggoth.c)
    target_link_libraries(shoggoth_example PRIVATE shoggoth_sdk)
endif()

# ── Tests ──────────────────────────────────────────────────────────────────────

if(SHOGGOTH_BUILD_TESTS)
    enable_testing()
    add_executable(shoggoth_test tests/test_sdk.c)
    target_link_libraries(shoggoth_test PRIVATE shoggoth_sdk)
    add_test(NAME shoggoth_sdk COMMAND shoggoth_test)
endif()
