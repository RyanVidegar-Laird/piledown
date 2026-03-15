// We need to forward routine registration from C to Rust
// to avoid the linker removing the static library.

#include <R.h>
#include <Rinternals.h>
#include <R_ext/Rdynload.h>

void R_init_piledownR_extendr(DllInfo *info);

void R_init_piledownR(DllInfo *info) {
    R_init_piledownR_extendr(info);
}
