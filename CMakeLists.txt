cmake_minimum_required(VERSION 3.21)
project(lumen VERSION 0.14.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)
set(CMAKE_INCLUDE_CURRENT_DIR ON)

find_package(Qt6 6.6 REQUIRED COMPONENTS
    Core Gui Qml Quick QuickControls2
    WaylandCompositor WaylandCompositorXdgShell
    Sql DBus Network)
find_package(Qt6 REQUIRED COMPONENTS Test)

qt_standard_project_setup(REQUIRES 6.6)

# ---------------------------------------------------------------------------
# lumen-daemon: the resident service. It owns the compositor and every session,
# shows no window, and is managed as a systemd user unit.
# ---------------------------------------------------------------------------
set(LUMEN_DAEMON_SOURCES
    src/daemon/main.cpp
    src/compositor.cpp
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
)
qt_add_qml_module(lumen
    URI Lumen
    VERSION 1.0
    QML_FILES
        qml/Main.qml
        qml/Theme.qml
        qml/SessionSidebar.qml
        qml/ControlBar.qml
        qml/SessionCanvas.qml
        qml/AnnotationOverlay.qml
        qml/FeedbackPanel.qml
        qml/FeedbackComposer.qml
        qml/SettingsPanel.qml
)
target_include_directories(lumen PRIVATE src)
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
    target_link_libraries(${name} PRIVATE Qt6::Core Qt6::Sql Qt6::DBus Qt6::Test Qt6::WaylandCompositor)
    add_test(NAME ${name} COMMAND ${name})
endfunction()

lumen_test(test_config src/config.cpp)
lumen_test(test_feedbackstore src/feedbackstore.cpp)
lumen_test(test_sessionmodel src/config.cpp src/session.cpp src/sessionmanager.cpp src/compositor.cpp)
lumen_test(test_accessibility src/accessibility.cpp)
lumen_test(test_integration src/compositor.cpp)

# ---------------------------------------------------------------------------
# The compositor spike is kept as a live regression test of the one
# architectural bet the project rests on.
# ---------------------------------------------------------------------------
qt_add_executable(lumen-spike src/spike/main.cpp)
qt_add_qml_module(lumen-spike
    URI LumenSpike
    VERSION 1.0
    QML_FILES src/spike/Spike.qml
)
target_compile_definitions(lumen-spike PRIVATE
    SPIKE_QML_PATH="${CMAKE_CURRENT_SOURCE_DIR}/src/spike/Spike.qml"
    FIXTURE_BIN="$<TARGET_FILE:qt_fixture>"
    FIXTURE_QML="${CMAKE_CURRENT_SOURCE_DIR}/src/spike/fixture.qml"
)
target_link_libraries(lumen-spike PRIVATE
    Qt6::Core Qt6::Gui Qt6::Qml Qt6::Quick
    Qt6::WaylandCompositor Qt6::Test
)

qt_add_executable(qt_fixture src/spike/qt_fixture.cpp)
target_link_libraries(qt_fixture PRIVATE Qt6::Core Qt6::Gui Qt6::Qml Qt6::Quick)

# The accessibility fixture: named controls whose tree an agent can read and act
# on, reused from the project's own test assets.
qt_add_executable(qt_a11y_fixture tests/fixtures/qt_app.cpp)
target_link_libraries(qt_a11y_fixture PRIVATE Qt6::Core Qt6::Gui Qt6::Qml Qt6::Quick)
