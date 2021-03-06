extern crate js;
extern crate libc;

use std::sync::mpsc::{Sender, Receiver};
use std::sync::mpsc;
use std::sync::Arc;
use std::ffi::CStr;
use std::ffi;
use std::ptr;
use std::str;
use std::mem;
use libc::{c_uint, c_void};

use js::{JSCLASS_RESERVED_SLOTS_MASK,JSCLASS_RESERVED_SLOTS_SHIFT,JSCLASS_GLOBAL_SLOT_COUNT,JSCLASS_IS_GLOBAL,JSCLASS_IMPLEMENTS_BARRIERS};
use js::jsapi::JS_GlobalObjectTraceHook;
use js::jsapi::{CallArgs,CompartmentOptions,OnNewGlobalHookOption,Rooted,Value};
use js::jsapi::{JS_DefineFunction,JS_Init,JS_NewGlobalObject, JS_InitStandardClasses,JS_EncodeStringToUTF8, JS_ReportPendingException, JS_BufferIsCompilableUnit, JS_DestroyContext, JS_DestroyRuntime, JS_ShutDown, CurrentGlobalOrNull, JS_ReportError, JS_SetPrivate};
use js::jsapi::{JSAutoCompartment,JSAutoRequest,JSContext,JSClass};
use js::jsapi::{JS_SetGCParameter, JSGCParamKey, JSGCMode};
use js::jsapi::{RootedValue, RootedObject, HandleObject, HandleValue};
use js::jsval::UndefinedValue;
use js::rust::Runtime;

const JSCLASS_HAS_PRIVATE: c_uint = 1 << 0;

static CLASS: &'static JSClass = &JSClass {
  name: b"global\0" as *const u8 as *const libc::c_char,
  flags: JSCLASS_IS_GLOBAL | JSCLASS_IMPLEMENTS_BARRIERS | JSCLASS_HAS_PRIVATE | ((JSCLASS_GLOBAL_SLOT_COUNT & JSCLASS_RESERVED_SLOTS_MASK) << JSCLASS_RESERVED_SLOTS_SHIFT),
  addProperty: None,
  delProperty: None,
  getProperty: None,
  setProperty: None,
  enumerate: None,
  resolve: None,
  convert: None,
  finalize: None,
  call: None,
  hasInstance: None,
  construct: None,
  trace: Some(JS_GlobalObjectTraceHook),
  reserved: [0 as *mut _; 25]
};

struct JSOptions {
  interactive: bool,
  disable_baseline: bool,
  disable_ion: bool,
  disable_asmjs: bool,
  disable_native_regexp: bool,
  script: String,
}

pub struct SMWorker<T> {
  ac: JSAutoCompartment,
  ar: JSAutoRequest,
  cx: *mut JSContext,
  runtime: Runtime,
  cb: T,
  tx: Sender<String>,
  rx: Receiver<String>
}

pub trait Function {
  fn callback(&self, message: String);
}

impl<T> SMWorker<T> where T: Function {
  pub fn execute(&self, label: String, script: String) -> Result<bool, &'static str> {
    let cx = self.cx;
    let global = unsafe { CurrentGlobalOrNull(cx) };
    let global_root = Rooted::new(cx, global);
    let global = global_root.handle();
    match self.runtime.evaluate_script(global, script, label, 1) {
      Err(_) => unsafe { JS_ReportPendingException(cx); Err("Uncaught exception during script execution") },
      _ => Ok(true)
    }
  }

  pub fn new(mut cb: T) -> SMWorker<T> {

  let (tx, rx): (Sender<String>, Receiver<String>) = mpsc::channel();

  unsafe {
    JS_Init();
  }

  let mut runtime = Runtime::new();
  let cx = runtime.cx();
  let h_option = OnNewGlobalHookOption::FireOnNewGlobalHook;
  let c_option = CompartmentOptions::default();
  let ar = JSAutoRequest::new(cx);
  let global = unsafe { JS_NewGlobalObject(cx, CLASS, ptr::null_mut(), h_option, &c_option) };
  let global_root = Rooted::new(cx, global);
  let global = global_root.handle();
  let ac = JSAutoCompartment::new(cx, global.get());
  let mut worker = SMWorker { ac: ac, ar: ar, cx: cx, runtime: runtime, cb: cb, tx: tx, rx: rx };
  let cb_ptr: *mut c_void = &mut worker.cb as *mut _ as *mut c_void;

  unsafe {
    JS_SetPrivate(global.get(), cb_ptr);
    JS_SetGCParameter(worker.runtime.rt(), JSGCParamKey::JSGC_MODE, JSGCMode::JSGC_MODE_INCREMENTAL as u32);
    JS_InitStandardClasses(cx, global);
    let function = JS_DefineFunction(cx, global, b"_send\0".as_ptr() as *const libc::c_char,
                                     Some(_send), 1, 0);
    assert!(!function.is_null());
  }

  worker
}

}

unsafe extern "C" fn _send(context: *mut JSContext, argc: u32, vp: *mut Value) -> bool {
  let args = CallArgs::from_vp(vp, argc);

  if args._base.argc_ != 1 {
    JS_ReportError(context, b"_send() requires exactly 1 argument\0".as_ptr() as *const libc::c_char);
    return false;
  }

  let arg = args.get(0);
  let js = js::rust::ToString(context, arg);
  let message_root = Rooted::new(context, js);
  let message = JS_EncodeStringToUTF8(context, message_root.handle());
  let message = CStr::from_ptr(message);
  /*let cb = recv_cb.unwrap();
  cb(str::from_utf8(message.to_bytes()).unwrap());*/
  println!("{}", str::from_utf8(message.to_bytes()).unwrap());

  args.rval().set(UndefinedValue());
  return true;
}
