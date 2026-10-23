# The published pow: licence

`src/numerics/published_pow.rs` is a translation into Rust of the power function `pow.c`, and
`src/numerics/pow_data.rs` holds the numbers of `pow_data.c` and `exp_data.c`: the
double-precision x^y of the Arm Optimized Routines (Szabolcs Nagy, Arm Limited, 2018), as musl
v1.2.6 carries it. Those files carry "Copyright (c) 2018, Arm Limited." and
"SPDX-License-Identifier: MIT"; musl's COPYRIGHT covers them under its MIT licence, whose notice
follows and must stay with these copies:

----------------------------------------------------------------------
Copyright © 2005-2020 Rich Felker, et al.

Permission is hereby granted, free of charge, to any person obtaining
a copy of this software and associated documentation files (the
"Software"), to deal in the Software without restriction, including
without limitation the rights to use, copy, modify, merge, publish,
distribute, sublicense, and/or sell copies of the Software, and to
permit persons to whom the Software is furnished to do so, subject to
the following conditions:

The above copyright notice and this permission notice shall be
included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
----------------------------------------------------------------------

Much of musl's math library code is Copyright © 2017-2018 Arm Limited and labelled as such in
comments in the individual source files (musl's COPYRIGHT), as these are.
