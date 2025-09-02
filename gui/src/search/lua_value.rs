use entrace_core::{EnValue, EnValueRef};

pub struct LuaValue(pub EnValue);
pub struct LuaValueRef<'a>(pub EnValueRef<'a>);
pub struct LuaValueRefRef<'a>(pub &'a EnValueRef<'a>);
impl mlua::IntoLua for LuaValue {
    fn into_lua(self, lua: &mlua::Lua) -> mlua::Result<mlua::Value> {
        match self.0 {
            EnValue::String(q) => q.into_lua(lua),
            EnValue::Bool(q) => q.into_lua(lua),
            EnValue::Bytes(q) => q.into_lua(lua),
            EnValue::Float(q) => q.into_lua(lua),
            EnValue::U64(q) => q.into_lua(lua),
            EnValue::I64(q) => q.into_lua(lua),
            EnValue::U128(q) => q.into_lua(lua),
            EnValue::I128(q) => q.into_lua(lua),
        }
    }
}
impl<'a> mlua::IntoLua for LuaValueRef<'a> {
    fn into_lua(self, lua: &mlua::Lua) -> mlua::Result<mlua::Value> {
        match self.0 {
            EnValueRef::String(q) => q.into_lua(lua),
            EnValueRef::Bool(q) => q.into_lua(lua),
            EnValueRef::Bytes(q) => q.into_lua(lua),
            EnValueRef::Float(q) => q.into_lua(lua),
            EnValueRef::U64(q) => q.into_lua(lua),
            EnValueRef::I64(q) => q.into_lua(lua),
            EnValueRef::U128(q) => q.into_lua(lua),
            EnValueRef::I128(q) => q.into_lua(lua),
        }
    }
}
impl<'a> mlua::IntoLua for LuaValueRefRef<'a> {
    fn into_lua(self, lua: &mlua::Lua) -> mlua::Result<mlua::Value> {
        match self.0 {
            EnValueRef::String(q) => q.into_lua(lua),
            EnValueRef::Bool(q) => q.into_lua(lua),
            EnValueRef::Bytes(q) => q.into_lua(lua),
            EnValueRef::Float(q) => q.into_lua(lua),
            EnValueRef::U64(q) => q.into_lua(lua),
            EnValueRef::I64(q) => q.into_lua(lua),
            EnValueRef::U128(q) => q.into_lua(lua),
            EnValueRef::I128(q) => q.into_lua(lua),
        }
    }
}
