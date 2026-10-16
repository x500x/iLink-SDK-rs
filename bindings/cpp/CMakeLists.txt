cmake_minimum_required(VERSION 3.15)

project(ilink-sdk
    VERSION 2.4.10
    DESCRIPTION "C++ bindings for the Rust iLink (Weixin) SDK"
    LANGUAGES CXX)

include(GNUInstallDirs)
include(CMakePackageConfigHelpers)

option(ILINK_SDK_BUILD_EXAMPLE "Build the C++ example" OFF)

# ---------------------------------------------------------------------------
# Locate the native library
#
# The C++ bindings are header-only; the only build artifact they need is the
# shared library produced by `cargo build --release`. We look for it in this
# order so both a source checkout and an installed SDK work:
#   1. ILINK_SDK_LIBRARY  (an explicit file)
#   2. ILINK_SDK_LIB_DIR  (a directory containing it)
#   3. ../../target/release relative to this file (source checkout)
#   4. the system loader path (found by the linker at link time)
# ---------------------------------------------------------------------------
find_path(ILINK_SDK_INCLUDE_DIR
    NAMES ilink_sdk.h
    HINTS "${CMAKE_CURRENT_LIST_DIR}/../../include"
    DOC "Directory containing ilink_sdk.h")

if(NOT ILINK_SDK_LIBRARY)
    if(DEFINED ILINK_SDK_LIB_DIR)
        find_library(ILINK_SDK_LIBRARY
            NAMES ilink_sdk libilink_sdk
            PATHS "${ILINK_SDK_LIB_DIR}"
            NO_DEFAULT_PATH)
    else()
        find_library(ILINK_SDK_LIBRARY
            NAMES ilink_sdk libilink_sdk
            HINTS "${CMAKE_CURRENT_LIST_DIR}/../../target/release"
            DOC "The iLink SDK import/static library")
    endif()
endif()

# Also locate the runtime library (the .dll/.so/.dylib) so it can be installed
# next to the import library. On MSVC the two differ (ilink_sdk.dll.lib is the
# import library, ilink_sdk.dll is the runtime); elsewhere find_library returns
# the runtime library directly.
if(WIN32)
    find_file(ILINK_SDK_RUNTIME_LIBRARY
        NAMES ilink_sdk.dll
        HINTS
            "${CMAKE_CURRENT_LIST_DIR}/../../target/release"
            "${ILINK_SDK_LIB_DIR}"
        DOC "The iLink SDK runtime DLL")
else()
    find_library(ILINK_SDK_RUNTIME_LIBRARY
        NAMES ilink_sdk libilink_sdk
        HINTS
            "${CMAKE_CURRENT_LIST_DIR}/../../target/release"
            "${ILINK_SDK_LIB_DIR}"
        DOC "The iLink SDK runtime shared library")
endif()

add_library(ilink-sdk INTERFACE)
add_library(ilink::ilink ALIAS ilink-sdk)
set_target_properties(ilink-sdk PROPERTIES EXPORT_NAME ilink)

target_include_directories(ilink-sdk INTERFACE
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}>
    $<INSTALL_INTERFACE:${CMAKE_INSTALL_INCLUDEDIR}/ilink-sdk>)
target_compile_features(ilink-sdk INTERFACE cxx_std_17)

# The C++ headers include the C ABI header (ilink_sdk.h), which lives in the
# repository's include/ directory (or next to the installed headers).
if(ILINK_SDK_INCLUDE_DIR)
    target_include_directories(ilink-sdk INTERFACE
        $<BUILD_INTERFACE:${ILINK_SDK_INCLUDE_DIR}>)
endif()

# The headers contain UTF-8 text; tell MSVC not to interpret them as the
# system code page.
if(MSVC)
    target_compile_options(ilink-sdk INTERFACE /utf-8)
endif()

if(ILINK_SDK_LIBRARY)
    # The build tree links the import/static library directly. The installed
    # package resolves the library itself in ilink-sdk-config.cmake, so the
    # export stays relocatable.
    target_link_libraries(ilink-sdk INTERFACE
        $<BUILD_INTERFACE:${ILINK_SDK_LIBRARY}>)
    if(APPLE)
        set_target_properties(ilink-sdk PROPERTIES
            INSTALL_RPATH "@loader_path/../lib")
    elseif(UNIX)
        set_target_properties(ilink-sdk PROPERTIES
            INSTALL_RPATH "$ORIGIN/../lib")
    endif()
else()
    message(WARNING
        "Could not find the iLink SDK shared library; the target will rely on "
        "the system loader path. Build it with `cargo build --release` or set "
        "ILINK_SDK_LIBRARY / ILINK_SDK_LIB_DIR.")
endif()

if(ILINK_SDK_BUILD_EXAMPLE)
    add_executable(ilink-cpp-example example.cpp)
    target_link_libraries(ilink-cpp-example PRIVATE ilink::ilink)
endif()

# ---------------------------------------------------------------------------
# Install: headers + a relocatable CMake package + pkg-config file
# ---------------------------------------------------------------------------
install(TARGETS ilink-sdk EXPORT ilink-sdk-targets)
install(FILES
    ilink_sdk.hpp
    ilink_types.hpp
    ilink_json.hpp
    DESTINATION "${CMAKE_INSTALL_INCLUDEDIR}/ilink-sdk")

if(ILINK_SDK_INCLUDE_DIR)
    install(FILES "${ILINK_SDK_INCLUDE_DIR}/ilink_sdk.h"
        DESTINATION "${CMAKE_INSTALL_INCLUDEDIR}/ilink-sdk")
endif()

if(ILINK_SDK_LIBRARY)
    # Install the import/static library (used at link time on MSVC).
    get_filename_component(_ilink_lib_name "${ILINK_SDK_LIBRARY}" NAME)
    install(FILES "${ILINK_SDK_LIBRARY}"
        DESTINATION "${CMAKE_INSTALL_LIBDIR}"
        RENAME "${_ilink_lib_name}")
endif()

if(ILINK_SDK_RUNTIME_LIBRARY AND NOT ILINK_SDK_RUNTIME_LIBRARY STREQUAL ILINK_SDK_LIBRARY)
    # On Windows the runtime DLL is separate from the import library; install it
    # too so applications can load it at run time.
    get_filename_component(_ilink_runtime_name "${ILINK_SDK_RUNTIME_LIBRARY}" NAME)
    install(FILES "${ILINK_SDK_RUNTIME_LIBRARY}"
        DESTINATION "${CMAKE_INSTALL_LIBDIR}"
        RENAME "${_ilink_runtime_name}")
endif()

install(EXPORT ilink-sdk-targets
    FILE ilink-sdk-targets.cmake
    NAMESPACE ilink::
    DESTINATION "${CMAKE_INSTALL_LIBDIR}/cmake/ilink-sdk")
configure_package_config_file(
    "${CMAKE_CURRENT_SOURCE_DIR}/cmake/ilink-sdk-config.cmake.in"
    "${CMAKE_CURRENT_BINARY_DIR}/ilink-sdk-config.cmake"
    INSTALL_DESTINATION "${CMAKE_INSTALL_LIBDIR}/cmake/ilink-sdk")

write_basic_package_version_file(
    "${CMAKE_CURRENT_BINARY_DIR}/ilink-sdk-config-version.cmake"
    VERSION ${PROJECT_VERSION}
    COMPATIBILITY SameMajorVersion)

install(FILES
    "${CMAKE_CURRENT_BINARY_DIR}/ilink-sdk-config.cmake"
    "${CMAKE_CURRENT_BINARY_DIR}/ilink-sdk-config-version.cmake"
    DESTINATION "${CMAKE_INSTALL_LIBDIR}/cmake/ilink-sdk")

configure_file(
    "${CMAKE_CURRENT_SOURCE_DIR}/cmake/ilink-sdk.pc.in"
    "${CMAKE_CURRENT_BINARY_DIR}/ilink-sdk.pc"
    @ONLY)
install(FILES "${CMAKE_CURRENT_BINARY_DIR}/ilink-sdk.pc"
    DESTINATION "${CMAKE_INSTALL_LIBDIR}/pkgconfig")
