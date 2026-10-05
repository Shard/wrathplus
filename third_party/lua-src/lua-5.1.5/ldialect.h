/*
** Runtime selection between benilla's default Lua 5.0 compatibility dialect
** and stock Lua 5.1 syntax/VM behavior.
*/

#ifndef ldialect_h
#define ldialect_h

#include "lobject.h"
#include "lstate.h"
#include "lstring.h"
#include "ltable.h"


#define BENILLA_WRATH51_REGISTRY_KEY  "benilla.wrath51"


/* Read the shared registry directly: parser and VM paths must not touch the
** public Lua stack. Only the boolean value true enables the Wrath dialect. */
static int benilla_iswrath51 (lua_State *L) {
  const TValue *flag = luaH_getstr(hvalue(registry(L)),
                                   luaS_newliteral(L,
                                                   BENILLA_WRATH51_REGISTRY_KEY));
  return ttisboolean(flag) && bvalue(flag);
}


#endif