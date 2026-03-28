rule download_fasta:
    output:
        "workflow/results/reference/GRCh38.primary_assembly.genome.fa.gz",
    log:
        "workflow/results/logs/download_fasta.log",
    params:
        url=config["reference"]["fasta_url"],
    shell:
        "curl -fL -o {output} {params.url} 2> {log}"


rule download_gtf:
    output:
        "workflow/results/reference/gencode.v{release}.annotation.gtf.gz",
    log:
        "workflow/results/logs/download_gtf_{release}.log",
    params:
        url=config["reference"]["gtf_url"],
    shell:
        "curl -fL -o {output} {params.url} 2> {log}"
