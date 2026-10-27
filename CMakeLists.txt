cmake_minimum_required(VERSION 3.21)
project(lumen VERSION 0.14.4 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)
set(CMAKE_INCLUDE_CURRENT_DIR ON)

find_package(Qt6 6.6 REQUIRED COMPONENTS
    Core Gui Qml Quick QuickControls2
    WaylandCompositor WaylandCompositorXdgShell
    Sql DBus Network Concurrent)
find_package(Qt6 REQUIRED COMPONENTS Test)

qt_standard_project_setup(REQUIRES 6.6)

# ---------------------------------------------------------------------------
# lumen-daemon: the resident service. It owns the compositor and every session,
# shows no window, and is managed as a systemd user unit.
# ---------------------------------------------------------------------------
# The compositor, shared by the daemon and the tests that host a client. It
# speaks wlr-layer-shell, which Qt does not ship, so the protocol's server glue
# is generated here from the vendored XML — once, which is why this is a
# library rather than a source list every target compiles for itself.
add_library(lumen_compositor STATIC src/compositor.cpp src/layershell.cpp src/xdgshellextras.cpp)
qt6_generate_wayland_protocol_server_sources(lumen_compositor
    FILES ${CMAKE_SOURCE_DIR}/protocols/wlr-layer-shell-unstable-v1.xml)
target_include_directories(lumen_compositor PUBLIC src ${CMAKE_CURRENT_BINARY_DIR})
target_link_libraries(lumen_compositor PUBLIC Qt6::Core Qt6::Gui Qt6::WaylandCompositor)

set(LUMEN_DAEMON_SOURCES
    src/daemon/main.cpp
    src/config.cpp
    src/session.cpp
    src/sessionmanager.cpp
    src/accessibility.cpp
    src/feedbackstore.cpp
    src/agentserver.cpp
    src/framestream.cpp
)

qt_add_executable(lumen-daemon ${LUMEN_DAEMON_SOURCES})
target_include_directories(lumen-daemon PRIVATE src)
target_link_libraries(lumen-daemon PRIVATE
    lumen_compositor
    Qt6::Core Qt6::Gui Qt6::Qml
    Qt6::WaylandCompositor Qt6::Sql Qt6::DBus
)
install(TARGETS lumen-daemon RUNTIME DESTINATION bin)

# ---------------------------------------------------------------------------
# lumen: the viewer and settings application. It renders frames the daemon sends
# and controls the daemon through systemd; it owns no sessions and no compositor.
# ---------------------------------------------------------------------------
qt_add_executable(lumen
    src/viewer/main.cpp
    src/viewer/daemonclient.cpp
    src/viewer/daemoncontrol.cpp
    src/viewer/frameprovider.cpp
    src/viewer/noteimageprovider.cpp
    src/viewer/theme.cpp
)
qt_add_qml_module(lumen
    URI Lumen
    VERSION 1.0
    QML_FILES
        qml/Main.qml
        qml/SessionSidebar.qml
        qml/ControlBar.qml
        qml/SessionCanvas.qml
        qml/AnnotationOverlay.qml
        qml/FeedbackPanel.qml
        qml/FeedbackComposer.qml
        qml/SettingsPanel.qml
        qml/ButtonSurface.qml
        qml/FieldSurface.qml
        qml/DialogSurface.qml
        qml/DialogFrame.qml
        qml/StatusDot.qml
        qml/IconButton.qml
        qml/Icon.qml
    SOURCES
        src/viewer/theme.h
)
target_include_directories(lumen PRIVATE src src/viewer)
target_link_libraries(lumen PRIVATE
    Qt6::Core Qt6::Gui Qt6::Qml Qt6::Quick Qt6::QuickControls2
    Qt6::Network Qt6::DBus
)
install(TARGETS lumen RUNTIME DESTINATION bin)

# ---------------------------------------------------------------------------
# The agent-facing CLI. It speaks to the running Lumen over a unix socket;
# there is no network surface anywhere in this project.
# ---------------------------------------------------------------------------
qt_add_executable(lumen-cli
    src/cli/main.cpp
)
target_include_directories(lumen-cli PRIVATE src)
target_link_libraries(lumen-cli PRIVATE Qt6::Core Qt6::Network)
install(TARGETS lumen-cli RUNTIME DESTINATION bin)

# ---------------------------------------------------------------------------
# Tests.
# ---------------------------------------------------------------------------
enable_testing()

# Each Qt test file carries its own QTEST_MAIN, so each is its own binary.
function(lumen_test name)
    qt_add_executable(${name} tests/${name}.cpp ${ARGN})
    target_include_directories(${name} PRIVATE src)
    target_link_libraries(${name} PRIVATE Qt6::Core Qt6::Sql Qt6::DBus Qt6::Test Qt6::WaylandCompositor Qt6::Concurrent)
    add_test(NAME ${name} COMMAND ${name})
endfunction()

lumen_test(test_config src/config.cpp)
lumen_test(test_feedbackstore src/feedbackstore.cpp)
lumen_test(test_sessionmodel src/config.cpp src/session.cpp src/sessionmanager.cpp src/accessibility.cpp)
target_link_libraries(test_sessionmodel PRIVATE lumen_compositor)
lumen_test(test_accessibility src/accessibility.cpp)
lumen_test(test_integration)
lumen_test(test_framerender)
target_link_libraries(test_integration PRIVATE lumen_compositor)
target_link_libraries(test_framerender PRIVATE lumen_compositor)


# ---------------------------------------------------------------------------
# The session client the integration and frame tests host. It is an ordinary
# Wayland application that knows nothing about Lumen: the title is the probe,
# changing on hover, press, release, click and key, so a test can tell exactly
# how far a synthesized input event travelled.
# ---------------------------------------------------------------------------
qt_add_executable(qt_fixture tests/fixtures/qt_fixture.cpp)
target_link_libraries(qt_fixture PRIVATE Qt6::Core Qt6::Gui Qt6::Qml Qt6::Quick)
