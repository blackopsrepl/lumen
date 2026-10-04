// The theme singleton is a C++ type, so the QML module generator needs to see
// its declaration in a translation unit of this target to emit the registration
// that makes `Theme` resolve in QML.

#include "theme.h"
