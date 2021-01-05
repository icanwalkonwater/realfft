//! # RealFFT: Real-to-complex FFT and complex-to-real iFFT based on RustFFT
//!
//! This library is a wrapper for RustFFT that enables faster computations when the input data is real.
//! It packs a 2N long real vector into an N long complex vector, which is transformed using a standard FFT.
//! It then post-processes the result to give only the first half of the complex spectrum, as an N+1 long complex vector.
//!
//! The iFFT goes through the same steps backwards, to transform an N+1 long complex spectrum to a 2N long real result.
//!
//! The speed increase compared to just converting the input to a 2N long complex vector
//! and using a 2N long FFT depends on the length f the input data.
//! The largest improvements are for long FFTs and for lengths over around 1000 elements there is an improvement of about a factor 2.
//! The difference shrinks for shorter lengths, and around 100 elements there is no longer any difference.  
//!
//! ## Why use real-to-complex fft?
//! ### Using a complex-to-complex fft
//! A simple way to get the fft of a rea values vector is to convert it to complex, and using a complex-to-complex fft.
//!
//! Let's assume `x` is a 6 element long real vector:
//! ```text
//! x = [x0r, x1r, x2r, x3r, x4r, x5r]
//! ```
//!
//! Converted to complex, using the notation `(xNr, xNi)` for the complex value `xN`, this becomes:
//! ```text
//! x_c = [(x0r, 0), (x1r, 0), (x2r, 0), (x3r, 0), (x4r, 0, (x5r, 0)]
//! ```
//!
//!
//! The general result of `X = FFT(x)` is:
//! ```text
//! X = [(X0r, X0i), (X1r, X1i), (X2r, X2i), (X3r, X3i), (X4r, X4i), (X5r, X5i)]
//! ```
//!
//! However, because our `x` was real-valued, some of this is redundant:
//! ```text
//! FFT(x) = [(X0r, 0), (X1r, X1i), (X2r, X2i), (X3r, 0), (X2r, -X2i), (X1r, -X1i)]
//! ```
//!
//! As we can see, the output contains a fair bit of redundant data. But it still takes time for the FFT to calculate these values. Converting the input data to complex also takes a little bit of time.
//!
//! ### real-to-complex
//! Using a real-to-complex fft removes the need for converting the input data to complex.
//! It also avoids caclulating the redundant output values.
//!
//! The result is:
//! ```text
//! RealFFT(x) = [(X0r, 0), (X1r, X1i), (X2r, X2i), (X3r, 0)]
//! ```
//!
//! This is the data layout output by the real-to-complex fft, and the one expected as input to the complex-to-real ifft.
//!
//! ## Scaling
//! RealFFT matches the behaviour of RustFFT and does not normalize the output of either FFT of iFFT. To get normalized results, each element must be scaled by `1/sqrt(length)`. If the processing involves both an FFT and an iFFT step, it is advisable to merge the two normalization steps to a single, by scaling by `1/length`.
//!
//! ## Documentation
//!
//! The full documentation can be generated by rustdoc. To generate and view it run:
//! ```text
//! cargo doc --open
//! ```
//!
//! ## Benchmarks
//!
//! To run a set of benchmarks comparing real-to-complex FFT with standard complex-to-complex, type:
//! ```text
//! cargo bench
//! ```
//! The results are printed while running, and are compiled into an html report containing much more details.
//! To view, open `target/criterion/report/index.html` in a browser.
//!
//! ## Example
//! Transform a vector, and then inverse transform the result.
//! ```
//! use realfft::{ComplexToReal, RealToComplex};
//! use rustfft::num_complex::Complex;
//! use rustfft::num_traits::Zero;
//!
//! // make dummy input vector, spectrum and output vectors
//! let mut indata = vec![0.0f64; 256];
//! let mut spectrum: Vec<Complex<f64>> = vec![Complex::zero(); 129];
//! let mut outdata: Vec<f64> = vec![0.0; 256];
//!
//! //create an FFT and forward transform the input data
//! let mut r2c = RealToComplex::<f64>::new(256).unwrap();
//! r2c.process(&mut indata, &mut spectrum).unwrap();
//!
//! // create an iFFT and inverse transform the spectum
//! let mut c2r = ComplexToReal::<f64>::new(256).unwrap();
//! c2r.process(&spectrum, &mut outdata).unwrap();
//! ```
//!
//! ## Compatibility
//!
//! The `realfft` crate requires rustc version 1.37 or newer.

use rustfft::num_complex::Complex;
use rustfft::num_traits::Zero;
use rustfft::FftPlanner;
use std::error;
use std::fmt;

type Res<T> = Result<T, Box<dyn error::Error>>;

/// Custom error returned by FFTs
#[derive(Debug)]
pub struct FftError {
    desc: String,
}

impl fmt::Display for FftError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.desc)
    }
}

impl error::Error for FftError {
    fn description(&self) -> &str {
        &self.desc
    }
}

impl FftError {
    pub fn new(desc: &str) -> Self {
        FftError {
            desc: desc.to_owned(),
        }
    }
}

/// An FFT that takes a real-valued input vector of length 2*N and transforms it to a complex
/// spectrum of length N+1.
pub struct RealToComplex<T> {
    sin_cos: Vec<(T, T)>,
    length: usize,
    fft: std::sync::Arc<dyn rustfft::Fft<T>>,
    buffer_out: Vec<Complex<T>>,
    scratch: Vec<Complex<T>>,
}

/// An FFT that takes a real-valued input vector of length 2*N and transforms it to a complex
/// spectrum of length N+1.
pub struct ComplexToReal<T> {
    sin_cos: Vec<(T, T)>,
    length: usize,
    fft: std::sync::Arc<dyn rustfft::Fft<T>>,
    buffer_in: Vec<Complex<T>>,
    scratch: Vec<Complex<T>>,
}

fn zip4<A, B, C, D>(
    a: A,
    b: B,
    c: C,
    d: D,
) -> impl Iterator<Item = (A::Item, B::Item, C::Item, D::Item)>
where
    A: IntoIterator,
    B: IntoIterator,
    C: IntoIterator,
    D: IntoIterator,
{
    a.into_iter()
        .zip(b.into_iter().zip(c.into_iter().zip(d)))
        .map(|(w, (x, (y, z)))| (w, x, y, z))
}

macro_rules! impl_r2c {
    ($ft:ty) => {
        impl RealToComplex<$ft> {
            /// Create a new RealToComplex FFT for input data of a given length. Returns an error if the length is not even.
            pub fn new(length: usize) -> Res<Self> {
                if length % 2 > 0 {
                    return Err(Box::new(FftError::new("Length must be even")));
                }
                let buffer_out = vec![Complex::zero(); length / 2 + 1];
                let mut sin_cos = Vec::with_capacity(length / 2);
                let pi = std::f64::consts::PI as $ft;
                for k in 0..length / 2 {
                    let sin = (k as $ft * pi / (length / 2) as $ft).sin();
                    let cos = (k as $ft * pi / (length / 2) as $ft).cos();
                    sin_cos.push((sin, cos));
                }
                let mut fft_planner = FftPlanner::<$ft>::new();
                let fft = fft_planner.plan_fft_forward(length / 2);
                let scratch = vec![Complex::zero(); fft.get_outofplace_scratch_len()];
                Ok(RealToComplex {
                    sin_cos,
                    length,
                    fft,
                    buffer_out,
                    scratch,
                })
            }

            /// Transform a vector of 2*N real-valued samples, storing the result in the N+1 element long complex output vector.
            /// The input buffer is used as scratch space, so the contents of input should be considered garbage after calling.
            pub fn process(&mut self, input: &mut [$ft], output: &mut [Complex<$ft>]) -> Res<()> {
                if input.len() != self.length {
                    return Err(Box::new(FftError::new(
                        format!(
                            "Wrong length of input, expected {}, got {}",
                            self.length,
                            input.len()
                        )
                        .as_str(),
                    )));
                }
                if output.len() != (self.length / 2 + 1) {
                    return Err(Box::new(FftError::new(
                        format!(
                            "Wrong length of output, expected {}, got {}",
                            self.length / 2 + 1,
                            input.len()
                        )
                        .as_str(),
                    )));
                }
                let fftlen = self.length / 2;
                //for (val, buf) in input.chunks(2).take(fftlen).zip(self.buffer_in.iter_mut()) {
                //    *buf = Complex::new(val[0], val[1]);
                //}
                let mut buf_in = unsafe {
                    let ptr = input.as_mut_ptr() as *mut Complex<$ft>;
                    let len = input.len();
                    std::slice::from_raw_parts_mut(ptr, len / 2)
                };

                // FFT and store result in buffer_out
                self.fft.process_outofplace_with_scratch(
                    &mut buf_in,
                    &mut self.buffer_out[0..fftlen],
                    &mut self.scratch,
                );

                self.buffer_out[fftlen] = self.buffer_out[0];

                for (&buf, &buf_rev, &(sin, cos), out) in zip4(
                    &self.buffer_out,
                    self.buffer_out.iter().rev(),
                    &self.sin_cos,
                    &mut output[..],
                ) {
                    let xr = 0.5
                        * ((buf.re + buf_rev.re) + cos * (buf.im + buf_rev.im)
                            - sin * (buf.re - buf_rev.re));
                    let xi = 0.5
                        * ((buf.im - buf_rev.im)
                            - sin * (buf.im + buf_rev.im)
                            - cos * (buf.re - buf_rev.re));
                    *out = Complex::new(xr, xi);
                }
                output[fftlen] = Complex::new(self.buffer_out[0].re - self.buffer_out[0].im, 0.0);
                Ok(())
            }
        }
    };
}
impl_r2c!(f64);
impl_r2c!(f32);

macro_rules! impl_c2r {
    ($ft:ty) => {
        /// Create a new ComplexToReal iFFT for output data of a given length. Returns an error if the length is not even.
        impl ComplexToReal<$ft> {
            pub fn new(length: usize) -> Res<Self> {
                if length % 2 > 0 {
                    return Err(Box::new(FftError::new("Length must be even")));
                }
                let buffer_in = vec![Complex::zero(); length / 2];
                let mut sin_cos = Vec::with_capacity(length / 2);
                let pi = std::f64::consts::PI as $ft;
                for k in 0..length / 2 {
                    let sin = (k as $ft * pi / (length / 2) as $ft).sin();
                    let cos = (k as $ft * pi / (length / 2) as $ft).cos();
                    sin_cos.push((sin, cos));
                }
                let mut fft_planner = FftPlanner::<$ft>::new();
                let fft = fft_planner.plan_fft_inverse(length / 2);
                let scratch = vec![Complex::zero(); fft.get_outofplace_scratch_len()];
                Ok(ComplexToReal {
                    sin_cos,
                    length,
                    fft,
                    buffer_in,
                    scratch,
                })
            }

            /// Transform a complex spectrum of N+1 values and store the real result in the 2*N long output.
            pub fn process(&mut self, input: &[Complex<$ft>], output: &mut [$ft]) -> Res<()> {
                if input.len() != (self.length / 2 + 1) {
                    return Err(Box::new(FftError::new(
                        format!(
                            "Wrong length of input, expected {}, got {}",
                            self.length / 2 + 1,
                            input.len()
                        )
                        .as_str(),
                    )));
                }
                if output.len() != self.length {
                    return Err(Box::new(FftError::new(
                        format!(
                            "Wrong length of output, expected {}, got {}",
                            self.length,
                            input.len()
                        )
                        .as_str(),
                    )));
                }

                for (&buf, &buf_rev, &(sin, cos), fft_input) in zip4(
                    input,
                    input.iter().rev(),
                    &self.sin_cos,
                    &mut self.buffer_in[..],
                ) {
                    let xr = (buf.re + buf_rev.re)
                        - cos * (buf.im + buf_rev.im)
                        - sin * (buf.re - buf_rev.re);
                    let xi = (buf.im - buf_rev.im) + cos * (buf.re - buf_rev.re)
                        - sin * (buf.im + buf_rev.im);
                    *fft_input = Complex::new(xr, xi);
                }

                // FFT and store result in buffer_out
                let mut buf_out = unsafe {
                    let ptr = output.as_mut_ptr() as *mut Complex<$ft>;
                    let len = output.len();
                    std::slice::from_raw_parts_mut(ptr, len / 2)
                };
                self.fft.process_outofplace_with_scratch(
                    &mut self.buffer_in,
                    &mut buf_out,
                    &mut self.scratch,
                );
                Ok(())
            }
        }
    };
}
impl_c2r!(f64);
impl_c2r!(f32);

#[cfg(test)]
mod tests {
    use crate::{ComplexToReal, RealToComplex};
    use rustfft::num_complex::Complex;
    use rustfft::num_traits::Zero;
    use rustfft::FftPlanner;

    fn compare_complex(a: &[Complex<f64>], b: &[Complex<f64>], tol: f64) -> bool {
        a.iter().zip(b.iter()).fold(true, |eq, (val_a, val_b)| {
            eq && (val_a.re - val_b.re).abs() < tol && (val_a.im - val_b.im).abs() < tol
        })
    }

    fn compare_f64(a: &[f64], b: &[f64], tol: f64) -> bool {
        a.iter()
            .zip(b.iter())
            .fold(true, |eq, (val_a, val_b)| eq && (val_a - val_b).abs() < tol)
    }

    // Compare RealToComplex with standard FFT
    #[test]
    fn real_to_complex() {
        let mut indata = vec![0.0f64; 256];
        for (i, val) in indata.iter_mut().enumerate() {
            *val = i as f64;
        }
        let mut rustfft_check = indata
            .iter()
            .map(|val| Complex::from(val))
            .collect::<Vec<Complex<f64>>>();
        let mut fft_planner = FftPlanner::<f64>::new();
        let fft = fft_planner.plan_fft_forward(256);

        let mut r2c = RealToComplex::<f64>::new(256).unwrap();
        let mut out_a: Vec<Complex<f64>> = vec![Complex::zero(); 129];

        fft.process(&mut rustfft_check);
        r2c.process(&mut indata, &mut out_a).unwrap();
        assert!(compare_complex(
            &out_a[0..129],
            &rustfft_check[0..129],
            1.0e-9
        ));
    }

    // Compare ComplexToReal with standard iFFT
    #[test]
    fn complex_to_real() {
        let mut indata = vec![Complex::<f64>::zero(); 256];
        indata[0] = Complex::new(1.0, 0.0);
        indata[1] = Complex::new(1.0, 0.4);
        indata[255] = Complex::new(1.0, -0.4);
        indata[3] = Complex::new(0.3, 0.2);
        indata[253] = Complex::new(0.3, -0.2);
        let mut rustfft_check = indata.clone();

        let mut fft_planner = FftPlanner::<f64>::new();
        let fft = fft_planner.plan_fft_inverse(256);

        let mut c2r = ComplexToReal::<f64>::new(256).unwrap();
        let mut out_a: Vec<f64> = vec![0.0; 256];

        c2r.process(&indata[0..129], &mut out_a).unwrap();
        fft.process(&mut rustfft_check);

        let check_real = rustfft_check.iter().map(|val| val.re).collect::<Vec<f64>>();
        assert!(compare_f64(&out_a, &check_real, 1.0e-9));
    }

    // Compare RealToComplex with standard FFT
    #[test]
    fn real_to_complex_odd() {
        let mut indata = vec![0.0f64; 254];
        indata[0] = 1.0;
        indata[3] = 0.5;
        let mut rustfft_check = indata
            .iter()
            .map(|val| Complex::from(val))
            .collect::<Vec<Complex<f64>>>();
        let mut fft_planner = FftPlanner::<f64>::new();
        let fft = fft_planner.plan_fft_forward(254);

        let mut r2c = RealToComplex::<f64>::new(254).unwrap();
        let mut out_a: Vec<Complex<f64>> = vec![Complex::zero(); 128];

        fft.process(&mut rustfft_check);
        r2c.process(&mut indata, &mut out_a).unwrap();
        assert!(compare_complex(
            &out_a[0..128],
            &rustfft_check[0..128],
            1.0e-9
        ));
    }

    // Compare ComplexToReal with standard iFFT
    #[test]
    fn complex_to_real_odd() {
        let mut indata = vec![Complex::<f64>::zero(); 254];
        indata[0] = Complex::new(1.0, 0.0);
        indata[1] = Complex::new(1.0, 0.4);
        indata[253] = Complex::new(1.0, -0.4);
        indata[3] = Complex::new(0.3, 0.2);
        indata[251] = Complex::new(0.3, -0.2);
        let mut rustfft_check = indata.clone();

        let mut fft_planner = FftPlanner::<f64>::new();
        let fft = fft_planner.plan_fft_inverse(254);

        let mut c2r = ComplexToReal::<f64>::new(254).unwrap();
        let mut out_a: Vec<f64> = vec![0.0; 254];

        c2r.process(&indata[0..128], &mut out_a).unwrap();
        fft.process(&mut rustfft_check);
        let check_real = rustfft_check.iter().map(|val| val.re).collect::<Vec<f64>>();
        assert!(compare_f64(&out_a[0..128], &check_real[0..128], 1.0e-9));
    }
}
