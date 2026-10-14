<script lang="ts">
	let { size = 32 }: { size?: number } = $props();
</script>

<!-- A toucan in profile: dark head, white cheek, big two-tone beak. -->
<svg width={size} height={size} viewBox="0 0 64 64" aria-hidden="true">
	<defs>
		<linearGradient id="beak" x1="0" y1="0" x2="1" y2="1">
			<stop offset="0" stop-color="#ffc93c" />
			<stop offset="1" stop-color="#ff7a1a" />
		</linearGradient>
	</defs>
	<circle cx="22" cy="34" r="18" fill="#26231f" />
	<path d="M14 32c0-7 5-12 12-12 4 0 6 2 6 5v10c0 6-5 10-10 10-5 0-8-6-8-13z" fill="#f6f1e7" />
	<path d="M30 22c10-4 24-3 31 4 2 2 1 5-2 5H33c-3 0-5-2-5-4s1-4 2-5z" fill="url(#beak)" />
	<path d="M33 31h26c-5 6-15 9-24 8-3 0-5-2-5-4s1-4 3-4z" fill="#e8591a" />
	<circle cx="24" cy="27" r="3.4" fill="#2a8dff" />
	<circle cx="24" cy="27" r="1.6" fill="#0b0b0b" />
</svg>
