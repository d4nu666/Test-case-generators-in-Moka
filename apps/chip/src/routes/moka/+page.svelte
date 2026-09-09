<script lang="ts">
  import { writable } from 'svelte/store';
  import { browser } from '$app/environment';
  import Editor from '$lib/components/Editor.svelte';
  import type { LtLResult, MarkerData } from 'chip-wasm';
  import Nav from '$lib/components/Nav.svelte';
  import Network from '$lib/components/Network.svelte';
  import Icon from '~icons/heroicons/globe-europe-africa';
  import { mirage } from 'ayu';

  let program = `> x = 30
do
x >= 0 -> x := x-1
od
check F x = -1              // should hold
check F x = -2              // should not hold
check G x = -1              // should not hold
check ! F ! (x = -1)        // should not hold
check ! (true U ! (x = -1)) // should not hold
check G x >= -1             // should hold
check ! F ! (x >= -1)       // should hold
`;

  let result = writable<LtLResult>({
    parse_error: false,
    markers: [],
    ts_dot: '',
    ts_map: new Map(),
    buchi_dot: '',
    negated_nnf_ltl_property_str: '',
    buchi_property_dot: '',
    gbuchi_property_dot: '',
    kripke_str: '',
    product_ba_dot: '',
  });
  let verifications = writable<MarkerData[]>([]);

  let parseError = writable(false);

  const STATUS = ['idle', 'checking', 'checked', 'error'];
  type Status = (typeof STATUS)[number];
  let status = writable<Status>('idle');

  $: graphs = [
    { title: 'Kripke structure', dot: $result.kripke_str },
    { title: 'Buchi automaton', dot: $result.buchi_dot },
    {
      title: `Generalized Buchi property: ${$result.negated_nnf_ltl_property_str}`,
      dot: $result.gbuchi_property_dot,
    },
    {
      title: `Buchi property: ${$result.negated_nnf_ltl_property_str}`,
      dot: $result.buchi_property_dot,
    },
    { title: 'Product automaton', dot: $result.product_ba_dot },
  ];

  let hoveredNode: string | null = null;
  let hoveredMarker: number | null = null;

  let hoverMakers: MarkerData[] = [];

  $: if (typeof hoveredNode == 'number' || typeof hoveredNode == 'string') {
    const spans = $result.ts_map.get(hoveredNode.toString());
    if (spans) {
      hoverMakers = spans.map(
        (span): MarkerData => ({
          relatedInformation: [],
          tags: [],
          severity: 'Info',
          message: 'here',
          span,
        }),
      );
    } else {
      hoverMakers = [];
    }
  } else {
    hoverMakers = [];
  }
  $: highlightedNodes =
    (typeof hoveredMarker == 'number' && $result.markers[hoveredMarker]?.[1]) || [];

  $: if (browser) {
    const run = async () => {
      status.set('checked');
      parseError.set(false);
      const { default: init, parse_ltl } = await import('chip-wasm');
      await init();
      console.time('run wasm');
      const res = parse_ltl(program);
      console.timeEnd('run wasm');
      if (res.parse_error) parseError.set(true);
      result.set(res);
      if (res.markers.length > 0) {
        status.set('error');
      } else {
        status.set('checked');
      }
    };
    run().catch(console.error);
  }

  // the wasm side does the work. assigning to program re-triggers the check above by itself
  let preset = 'default';
  let generating = false;
  let generated: { seed: number; attempts: number } | null = null;
  let generateError: string | null = null;

  const generateProgram = async () => {
    generating = true;
    generateError = null;
    generated = null;
    try {
      const { default: init, generate_program } = await import('chip-wasm');
      await init();
      // seed from js so it stays a plain number, and so chip generate can reuse it
      const seed = Math.floor(Math.random() * 2 ** 32);
      // sampling blocks the page, let the button repaint as disabled first
      await new Promise((resolve) => setTimeout(resolve, 0));
      console.time('generate wasm');
      const res = generate_program(seed, preset);
      console.timeEnd('generate wasm');
      if (res.accepted) {
        program = res.program;
        generated = { seed: res.seed, attempts: res.attempts };
      } else {
        // a dead seed is about the params, not the browser. press it again
        generateError = `no program passed the filter in ${res.attempts} tries`;
      }
    } catch (e) {
      generateError = String(e);
    } finally {
      generating = false;
    }
  };

  const prepareDot = (dot: string) =>
    dot
      .trim()
      .replaceAll('\n\n\n', '\n\n')
      .replaceAll(/"\]\[shape="doublecircle"/g, `",color="${mirage.syntax.tag.hex()}"`)
      .replaceAll(/\[label=[^\]]+\]\[shape="point"]/g, '[label="",opacity=0]')
      .replaceAll(/\]\[shape/g, ',shape');

  let pauseGraphRendering = false;
</script>

<svelte:head>
  <title>Moka</title>
  <meta name="description" content="Moka" />
</svelte:head>

<Nav title="Moka" {Icon} />

<div class="relative grid grid-cols-2 grid-rows-[2fr_auto] bg-slate-800">
  <div class="flex min-h-0 flex-col">
    <div class="flex items-center space-x-3 bg-slate-900 px-3 py-1.5 text-sm text-white">
      <button
        on:click={generateProgram}
        disabled={generating}
        class="font-bold transition hover:text-slate-300 disabled:opacity-50"
      >
        {generating ? 'Generating...' : 'Generate'}
      </button>
      <select
        bind:value={preset}
        disabled={generating}
        class="rounded-sm bg-slate-800 px-1 py-0.5 text-xs"
        title="which parameters to generate from"
      >
        <option value="default">default</option>
        <option value="stress">stress</option>
      </select>
    </div>
    <div class="relative min-h-0 flex-1">
      <Editor
        bind:value={program}
        bind:hoveredMarker
        markers={[...$result.markers.map((m) => m[0]), ...$verifications, ...hoverMakers]}
      />
    </div>
  </div>
  <div class="flex flex-col text-white">
    {#if false}
      {#each graphs as { title, dot }}
        {#if dot}
          <div class="flex flex-1 flex-col p-2">
            <h2 class="text-xl font-bold">{title}</h2>
            <div class="grid flex-1 grid-cols-1 overflow-auto rounded-sm bg-slate-700">
              {#if dot.includes('digraph')}
                <div class="m-1 rounded-sm bg-slate-900">
                  <Network dot={prepareDot(dot)} highlight={['n20']} />
                </div>
              {:else}
                <div class="relative">
                  <pre class="absolute inset-0 overflow-auto p-4">{prepareDot(dot)}</pre>
                </div>
              {/if}
            </div>
          </div>
        {/if}
      {/each}
    {:else}
      <div class="relative flex-1">
        {#if pauseGraphRendering}
          <button
            class="absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 rounded-sm border px-3 py-2 text-lg font-bold transition hover:bg-slate-500/10"
            on:click={() => (pauseGraphRendering = false)}
          >
            Graph rendering pause. Click to enable
          </button>
        {:else}
          <Network bind:hoveredNode dot={$result.ts_dot} highlight={highlightedNodes} />
        {/if}
        <div
          class="absolute right-1 top-1 flex items-center space-x-1 text-sm opacity-20 transition hover:opacity-100"
        >
          <label for="pause-graph-rendering" class="cursor-pointer select-none"
            >Pause graph rendering</label
          >
          <input
            type="checkbox"
            name="pause-graph-rendering"
            id="pause-graph-rendering"
            bind:checked={pauseGraphRendering}
          />
        </div>
      </div>
    {/if}
  </div>
  <div
    class="col-span-2 flex items-center p-2 text-2xl text-white transition duration-500 {$parseError
      ? 'bg-purple-600'
      : {
          idle: 'bg-gray-500',
          checking: 'bg-yellow-500',
          checked: 'bg-green-500',
          error: 'bg-red-500',
        }[$status]}"
  >
    <span class="font-bold">
      {$parseError
        ? 'Parse error'
        : {
            idle: 'Idle',
            checking: 'Checking...',
            checked: 'Checked',
            error: 'Error',
          }[$status]}
    </span>
    <div class="flex-1"></div>
    <span class="text-base">
      {#if generateError}
        {generateError}
      {:else if generated}
        seed <b>{generated.seed}</b> &middot;
        {generated.attempts}
        {generated.attempts == 1 ? 'candidate' : 'candidates'} drawn
      {/if}
      <!-- {#if !$parseError && $state == 'checked'}
          {#if $result.is_fully_annotated}
            The program is <b>fully annotated</b>
          {:else}
            The program is <b><i>not</i> fully annotated</b>
          {/if}
        {/if} -->
    </span>
  </div>
</div>
