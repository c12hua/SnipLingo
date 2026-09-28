use image::RgbaImage;

/// 暗色背景与低对比度增强预处理
/// 对 VSCode/终端纯黑主题或水印背景下的浅色字体，在内存中做动态范围拉伸与边缘对比度锐化，提升 OCR 召回率
pub fn preprocess_image_for_ocr(img: &RgbaImage, enhance: bool) -> RgbaImage {
    if !enhance {
        return img.clone();
    }

    let width = img.width();
    let height = img.height();
    let count = (width as u64) * (height as u64);
    if count == 0 {
        return img.clone();
    }

    let mut min_l = 255u8;
    let mut max_l = 0u8;
    let mut sum_l: u64 = 0;

    for pixel in img.pixels() {
        let l = (0.299 * pixel[0] as f32 + 0.587 * pixel[1] as f32 + 0.114 * pixel[2] as f32) as u8;
        if l < min_l {
            min_l = l;
        }
        if l > max_l {
            max_l = l;
        }
        sum_l += l as u64;
    }

    let avg_l = (sum_l / count) as u8;
    let dynamic_range = max_l.saturating_sub(min_l);

    // 如果属于暗色主题
    if dynamic_range < 160 || avg_l < 110 {
        let mut processed = img.clone();
        let range = (dynamic_range as f32).max(1.0);
        let min_f = min_l as f32;

        for pixel in processed.pixels_mut() {
            for c in 0..3 {
                let normalized = ((pixel[c] as f32 - min_f) / range).clamp(0.0, 1.0);
                let enhanced = if normalized < 0.5 {
                    2.0 * normalized * normalized
                } else {
                    1.0 - 2.0 * (1.0 - normalized) * (1.0 - normalized)
                };
                pixel[c] = (enhanced * 255.0).round() as u8;
            }
        }
        processed
    } else {
        img.clone()
    }
}

/// 暗色背景与低对比度增强并直接转换为推理所需的 RGB 格式 (单趟转换，避免生成中间 RGBA 冗余副本)
pub fn preprocess_image_to_rgb(img: &RgbaImage, enhance: bool) -> image::RgbImage {
    let width = img.width();
    let height = img.height();
    let count = (width as u64) * (height as u64);
    if count == 0 || !enhance {
        return image::RgbImage::from_fn(width, height, |x, y| {
            let p = img.get_pixel(x, y);
            image::Rgb([p[0], p[1], p[2]])
        });
    }

    let mut min_l = 255u8;
    let mut max_l = 0u8;
    let mut sum_l: u64 = 0;

    for pixel in img.pixels() {
        let l = (0.299 * pixel[0] as f32 + 0.587 * pixel[1] as f32 + 0.114 * pixel[2] as f32) as u8;
        if l < min_l {
            min_l = l;
        }
        if l > max_l {
            max_l = l;
        }
        sum_l += l as u64;
    }

    let avg_l = (sum_l / count) as u8;
    let dynamic_range = max_l.saturating_sub(min_l);

    // 如果属于暗色主题或对比度不足，执行曲线增强
    if dynamic_range < 160 || avg_l < 110 {
        let range = (dynamic_range as f32).max(1.0);
        let min_f = min_l as f32;

        image::RgbImage::from_fn(width, height, |x, y| {
            let p = img.get_pixel(x, y);
            let mut rgb = [0u8; 3];
            for c in 0..3 {
                let normalized = ((p[c] as f32 - min_f) / range).clamp(0.0, 1.0);
                let enhanced_val = if normalized < 0.5 {
                    2.0 * normalized * normalized
                } else {
                    1.0 - 2.0 * (1.0 - normalized) * (1.0 - normalized)
                };
                rgb[c] = (enhanced_val * 255.0).round() as u8;
            }
            image::Rgb(rgb)
        })
    } else {
        image::RgbImage::from_fn(width, height, |x, y| {
            let p = img.get_pixel(x, y);
            image::Rgb([p[0], p[1], p[2]])
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn test_preprocess_disabled() {
        let img = RgbaImage::from_pixel(10, 10, Rgba([30, 30, 30, 255]));
        let result = preprocess_image_for_ocr(&img, false);
        assert_eq!(result.get_pixel(0, 0), &Rgba([30, 30, 30, 255]));
    }

    #[test]
    fn test_preprocess_dark_theme_stretch() {
        let mut img = RgbaImage::from_pixel(10, 10, Rgba([20, 20, 20, 255]));
        // 模拟暗色主题中的浅灰色字体
        img.put_pixel(5, 5, Rgba([80, 80, 80, 255]));
        let result = preprocess_image_for_ocr(&img, true);
        let bg_pixel = result.get_pixel(0, 0);
        let text_pixel = result.get_pixel(5, 5);
        // 背景应拉伸更暗，文字应拉伸更亮，对比度明显提升
        assert!(text_pixel[0] > bg_pixel[0]);
    }

    #[test]
    fn test_preprocess_image_to_rgb_single_pass() {
        let mut img = RgbaImage::from_pixel(100, 100, Rgba([30, 30, 30, 255]));
        img.put_pixel(50, 50, Rgba([120, 120, 120, 255]));
        let rgb_enhanced = preprocess_image_to_rgb(&img, true);
        assert_eq!(rgb_enhanced.width(), 100);
        assert_eq!(rgb_enhanced.height(), 100);
        assert!(rgb_enhanced.get_pixel(50, 50)[0] > rgb_enhanced.get_pixel(0, 0)[0]);

        // 测试 4K 尺寸 (3840x2160) 单趟处理性能与无崩溃
        let img_4k = RgbaImage::new(3840, 2160);
        let t_start = std::time::Instant::now();
        let rgb_4k = preprocess_image_to_rgb(&img_4k, false);
        let elapsed = t_start.elapsed();
        assert_eq!(rgb_4k.width(), 3840);
        assert_eq!(rgb_4k.height(), 2160);
        eprintln!("4K 图像单趟 RGB 转换耗时: {:?}", elapsed);
    }
}

