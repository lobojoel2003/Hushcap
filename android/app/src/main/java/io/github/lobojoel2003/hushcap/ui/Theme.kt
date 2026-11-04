package io.github.lobojoel2003.hushcap.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import io.github.lobojoel2003.hushcap.settings.ThemeMode

private val Brand = Color(0xFF2A6FDB)

private val LightColors = lightColorScheme(
    primary = Brand,
    onPrimary = Color.White,
    secondary = Color(0xFF8A5A00),
    background = Color(0xFFFBFBFD),
    surface = Color(0xFFFBFBFD),
    onBackground = Color(0xFF121418),
    onSurface = Color(0xFF121418),
)

private val DarkColors = darkColorScheme(
    primary = Color(0xFF8DB4FF),
    onPrimary = Color(0xFF00285E),
    secondary = Color(0xFFF5B841),
    background = Color(0xFF0F1115),
    surface = Color(0xFF0F1115),
    onBackground = Color(0xFFECEEF2),
    onSurface = Color(0xFFECEEF2),
)

/** Yellow on black: the classic, most legible caption style. */
private val HighContrastColors = darkColorScheme(
    primary = Color(0xFFFFE600),
    onPrimary = Color.Black,
    secondary = Color.White,
    background = Color.Black,
    surface = Color.Black,
    surfaceVariant = Color(0xFF1A1A1A),
    onBackground = Color(0xFFFFE600),
    onSurface = Color(0xFFFFE600),
    onSurfaceVariant = Color.White,
    outline = Color.White,
)

@Composable
fun colorsFor(mode: ThemeMode): ColorScheme = when (mode) {
    ThemeMode.LIGHT -> LightColors
    ThemeMode.DARK -> DarkColors
    ThemeMode.HIGH_CONTRAST -> HighContrastColors
    ThemeMode.SYSTEM -> if (isSystemInDarkTheme()) DarkColors else LightColors
}

@Composable
fun HushcapTheme(mode: ThemeMode, content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = colorsFor(mode), content = content)
}

/** Distinct, colour-blind-friendly speaker colours (Okabe–Ito based). */
fun speakerColor(speaker: Int, dark: Boolean): Color {
    val light = listOf(0xFF0072B2, 0xFFD55E00, 0xFF009E73, 0xFFCC79A7, 0xFF8A6D00, 0xFF56B4E9, 0xFF6A3D9A, 0xFF000000)
    val night = listOf(0xFF56B4E9, 0xFFFF8C42, 0xFF33D6A6, 0xFFF0A3CF, 0xFFF0E442, 0xFF9AD0F5, 0xFFC5A3FF, 0xFFFFFFFF)
    val palette = if (dark) night else light
    return Color(palette[(speaker - 1).mod(palette.size)])
}
